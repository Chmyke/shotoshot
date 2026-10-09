use crate::*;
use windows::{
    Win32::{
        Foundation::*,
        System::{ProcessStatus::*, Threading::*},
        UI::WindowsAndMessaging::*,
    },
    core::*,
};
pub const COMPLETE: u32 = WM_APP + 50;
pub struct Diagnostic {
    cpu_start: u64,
    start: std::time::Instant,
}
pub fn editor_probe() -> Result<()> {
    let start = std::time::Instant::now();
    let image = capture_desktop()?;
    let capture_ms = start.elapsed().as_secs_f64() * 1000.;
    let (width, height) = (image.width, image.height);
    let _session = crate::session::Session::immediate(image)?;
    let ready_ms = start.elapsed().as_secs_f64() * 1000.;
    std::fs::write(
        "target/editor-diagnostics.txt",
        format!("{width} {height} {capture_ms} {ready_ms}"),
    )
    .map_err(|e| Error::new(E_FAIL, e.to_string()))
}
/// CONTRACT: a real editor process paints its first frame within this delay of its creation.
/// Measured on the reference desktop (three 1920×1080 screens): 130 ms, of which Windows takes
/// 42 ms to copy the desktop and 13 ms to show the window; waiting for a Direct3D device first
/// cost 272 ms (`docs/contexte/audit-latence-2026-10-01.md`).
const FIRST_FRAME_BUDGET_MS: f64 = 200.;
const FIRST_FRAME_REPORT: &str = "target/first-frame-diagnostics.txt";
/// Editor run by the resident diagnostic (`--first-frame-diagnostic`): the real capture path,
/// timed from process creation, closed as soon as it has reported.
#[derive(Default)]
pub struct FirstFrame {
    first_ms: Option<f64>,
    requested: Option<i64>,
}
impl FirstFrame {
    pub fn requested(&mut self, ticks: usize) {
        self.requested = Some(ticks as i64);
    }
    /// Call after each painted frame. True once the report is written: the first frame, then
    /// the first one drawn by the hardware renderer.
    pub fn painted(&mut self, hardware: bool) -> Result<bool> {
        unsafe {
            windows::Win32::Graphics::Gdi::GdiFlush().ok()?;
        }
        if let Some(start) = self.requested {
            let elapsed = elapsed_ticks_ms(start)?;
            std::fs::write("target/warm-frame-diagnostics.txt", elapsed.to_string())
                .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
            return Ok(true);
        }
        let now = since_creation_ms()?;
        let first = *self.first_ms.get_or_insert(now);
        if !hardware {
            return Ok(false);
        }
        std::fs::write(FIRST_FRAME_REPORT, format!("{first} {now}"))
            .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
        Ok(true)
    }
}
pub(crate) fn performance_ticks() -> Result<i64> {
    let mut ticks = 0;
    unsafe {
        windows::Win32::System::Performance::QueryPerformanceCounter(&mut ticks)?;
    }
    Ok(ticks)
}
pub(crate) fn elapsed_ticks_ms(start: i64) -> Result<f64> {
    let mut frequency = 0;
    unsafe {
        windows::Win32::System::Performance::QueryPerformanceFrequency(&mut frequency)?;
    }
    Ok((performance_ticks()? - start) as f64 * 1000. / frequency as f64)
}
fn since_creation_ms() -> Result<f64> {
    // FILETIME counts 100 ns ticks from 1601; the Unix epoch is this many ticks later.
    const UNIX_EPOCH_TICKS: u64 = 116_444_736_000_000_000;
    let (creation, _) = process_times()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
    let now_ticks = UNIX_EPOCH_TICKS + (now.as_nanos() / 100) as u64;
    Ok(now_ticks.saturating_sub(creation) as f64 / 10000.)
}
fn median(samples: &[f64]) -> f64 {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[sorted.len() / 2]
}
/// Creation time and consumed CPU time of this process, in 100 ns ticks.
fn process_times() -> Result<(u64, u64)> {
    unsafe {
        let mut creation = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )?;
        let ticks = |t: FILETIME| (t.dwHighDateTime as u64) << 32 | t.dwLowDateTime as u64;
        Ok((ticks(creation), ticks(kernel) + ticks(user)))
    }
}
fn cpu() -> Result<u64> {
    Ok(process_times()?.1)
}
fn memory() -> Result<usize> {
    unsafe {
        let mut counters = PROCESS_MEMORY_COUNTERS {
            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            ..Default::default()
        };
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )?;
        Ok(counters.WorkingSetSize)
    }
}
fn private_bytes() -> Result<usize> {
    unsafe {
        let mut counters = PROCESS_MEMORY_COUNTERS_EX {
            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
            ..Default::default()
        };
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            (&mut counters as *mut PROCESS_MEMORY_COUNTERS_EX).cast(),
            counters.cb,
        )?;
        Ok(counters.PrivateUsage)
    }
}
impl Diagnostic {
    pub fn start() -> Result<Self> {
        let thread = unsafe { GetCurrentThreadId() };
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(2));
            unsafe {
                let _ = PostThreadMessageW(thread, COMPLETE, WPARAM(0), LPARAM(0));
            }
        });
        Ok(Self {
            cpu_start: cpu()?,
            start: std::time::Instant::now(),
        })
    }
    pub fn finish(self) -> Result<()> {
        let idle_seconds = self.start.elapsed().as_secs_f64();
        let cpu_ms = (cpu()? - self.cpu_start) as f64 / 10000.;
        let idle_memory = memory()?;
        let idle_private = private_bytes()?;
        let mut samples = Vec::new();
        let mut ready_samples = Vec::new();
        let mut process_samples = Vec::new();
        let mut dimensions = (0, 0);
        let before = unsafe { GetGuiResources(GetCurrentProcess(), GR_GDIOBJECTS) };
        for _ in 0..5 {
            let start = std::time::Instant::now();
            let executable =
                std::env::current_exe().map_err(|e| Error::new(E_FAIL, e.to_string()))?;
            let status = std::process::Command::new(executable)
                .arg("--editor-diagnostic")
                .status()
                .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
            if !status.success() {
                return Err(Error::new(E_FAIL, "Editor process check failed"));
            }
            process_samples.push(start.elapsed().as_secs_f64() * 1000.);
            let values = std::fs::read_to_string("target/editor-diagnostics.txt")
                .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
            let numbers: std::result::Result<Vec<f64>, _> =
                values.split_whitespace().map(str::parse).collect();
            let numbers = numbers.map_err(|e| Error::new(E_FAIL, e.to_string()))?;
            if numbers.len() != 4 {
                return Err(Error::new(E_FAIL, "Invalid editor diagnostics"));
            }
            dimensions = (numbers[0] as u32, numbers[1] as u32);
            samples.push(numbers[2]);
            ready_samples.push(numbers[3]);
        }
        let after = unsafe { GetGuiResources(GetCurrentProcess(), GR_GDIOBJECTS) };
        if before != after {
            return Err(Error::new(E_FAIL, "GDI resources leaked during capture"));
        }
        let mut first_frames = Vec::new();
        let mut hardware_frames = Vec::new();
        for _ in 0..5 {
            let executable =
                std::env::current_exe().map_err(|e| Error::new(E_FAIL, e.to_string()))?;
            let status = std::process::Command::new(executable)
                .arg("--first-frame-diagnostic")
                .status()
                .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
            if !status.success() {
                return Err(Error::new(E_FAIL, "First-frame check failed"));
            }
            let values = std::fs::read_to_string(FIRST_FRAME_REPORT)
                .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
            let numbers: std::result::Result<Vec<f64>, _> =
                values.split_whitespace().map(str::parse).collect();
            let numbers = numbers.map_err(|e| Error::new(E_FAIL, e.to_string()))?;
            if numbers.len() != 2 {
                return Err(Error::new(E_FAIL, "Invalid first-frame diagnostics"));
            }
            first_frames.push(numbers[0]);
            hardware_frames.push(numbers[1]);
        }
        let json = format!(
            "{{\n  \"idle_seconds\": {idle_seconds:.3},\n  \"idle_cpu_ms\": {cpu_ms:.3},\n  \"idle_working_set_bytes\": {idle_memory},\n  \"capture_dimensions\": [{}, {}],\n  \"capture_ms\": {:?},\n  \"capture_and_editor_init_ms\": {ready_samples:?},\n  \"gdi_before\": {before},\n  \"gdi_after\": {after},\n  \"working_set_after_captures\": {}\n}}\n",
            dimensions.0,
            dimensions.1,
            samples,
            memory()?
        );
        let private_after_captures = private_bytes()?;
        crate::app::menu_probe()?;
        let final_private = private_bytes()?;
        let json = json.replace(
            "
}",
            &format!(
                ",
  \"worker_lifetime_ms\": {process_samples:?},
  \"first_frame_ms\": {first_frames:?},
  \"hardware_frame_ms\": {hardware_frames:?},
  \"idle_private_bytes\": {idle_private},
  \"private_bytes_after_captures\": {private_after_captures},
  \"private_bytes_after_menu\": {final_private}
}}"
            ),
        );
        // The figures are written before the contract is judged, so a failure can be read.
        std::fs::write("target/diagnostics.json", json)
            .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
        // CONTRACT: The resident must not retain a full virtual-desktop image after workers
        // exit, nor the notification menu's renderer after the menu closes.
        if final_private > idle_private + 8 * 1024 * 1024 {
            return Err(Error::new(
                E_FAIL,
                "Resident retained more than 8 MiB after editor workers exited and its menu closed",
            ));
        }
        if median(&first_frames) > FIRST_FRAME_BUDGET_MS {
            return Err(Error::new(
                E_FAIL,
                "The editor's first frame came later than its budget after process creation",
            ));
        }
        Ok(())
    }
}
