//! Explicit noninteractive probe of the same owner/child protocol used by the resident.
//! The origin is the parent's activation request; completion is first paint + GdiFlush.
use super::*;
use std::time::{Duration, Instant};
use windows::Win32::System::ProcessStatus::*;

const REPORT: &str = "target/warm-frame-diagnostics.txt";
fn io(error: std::io::Error) -> Error {
    Error::new(E_FAIL, error.to_string())
}
fn cpu(process: HANDLE) -> Result<u64> {
    let (mut creation, mut exit, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    unsafe {
        GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user)?;
    }
    let ticks = |t: FILETIME| ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64;
    Ok(ticks(kernel) + ticks(user))
}
fn memory(process: HANDLE) -> Result<(usize, usize)> {
    let mut counters = PROCESS_MEMORY_COUNTERS_EX {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        ..Default::default()
    };
    unsafe {
        GetProcessMemoryInfo(
            process,
            (&mut counters as *mut PROCESS_MEMORY_COUNTERS_EX).cast(),
            counters.cb,
        )?;
    }
    Ok((counters.WorkingSetSize, counters.PrivateUsage))
}
fn ready(owner: HWND, worker: &mut editor_process::EditorProcess) -> Result<HWND> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let mut message = MSG::default();
        while unsafe { PeekMessageW(&mut message, Some(owner), 0, 0, PM_REMOVE) }.as_bool() {
            if message.message == editor_process::READY && message.wParam.0 == worker.pid() as usize
            {
                let window = HWND(message.lParam.0 as _);
                worker.ready(window);
                if unsafe { IsWindowVisible(window) }.as_bool() {
                    return Err(Error::new(
                        E_FAIL,
                        "Prepared editor became visible before activation",
                    ));
                }
                let style = unsafe { GetWindowLongPtrW(window, GWL_EXSTYLE) } as u32;
                if style & WS_EX_APPWINDOW.0 == 0 || style & WS_EX_TOOLWINDOW.0 != 0 {
                    return Err(Error::new(E_FAIL, "Editor is excluded from Alt+Tab"));
                }
                return Ok(window);
            }
        }
        if worker.exit_code()?.is_some() || Instant::now() >= deadline {
            return Err(Error::new(E_FAIL, "Prepared editor did not become ready"));
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}
pub(super) fn run() -> Result<()> {
    let _mutex = crate::resources::OwnedMutex(unsafe {
        CreateMutexW(None, false, w!("Local\\Shotoshot.WarmDiagnostic"))?
    });
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        return Err(Error::new(E_FAIL, "Warm diagnostic already running"));
    }
    let owner = crate::resources::OwnedWindow(unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("STATIC"),
            w!("Shotoshot warm diagnostic"),
            WS_POPUP,
            -32000,
            -32000,
            1,
            1,
            None,
            None,
            None,
            None,
        )?
    });
    let mut reports = Vec::new();
    for live in [false, true] {
        let mut samples = Vec::new();
        let (mut working_set, mut private_bytes, mut idle_cpu_ms) = (0, 0, 0.);
        for index in 0..5 {
            let _ = std::fs::remove_file(REPORT);
            let mut worker = editor_process::EditorProcess::launch(
                owner.0,
                true,
                if live {
                    "--first-frame-diagnostic --live-diagnostic"
                } else {
                    "--first-frame-diagnostic"
                },
            )?;
            ready(owner.0, &mut worker)?;
            if index == 0 {
                (working_set, private_bytes) = memory(worker.handle())?;
                let before = cpu(worker.handle())?;
                std::thread::sleep(Duration::from_secs(2));
                idle_cpu_ms = cpu(worker.handle())?.saturating_sub(before) as f64 / 10000.;
                if std::path::Path::new(REPORT).exists() {
                    return Err(Error::new(E_FAIL, "Editor painted before activation"));
                }
            }
            let window = worker
                .request()
                .ok_or_else(|| Error::new(E_FAIL, "Editor failed to accept activation"))?;
            unsafe {
                let _ = AllowSetForegroundWindow(worker.pid());
            }
            let start = crate::diagnostics::performance_ticks()?;
            unsafe {
                PostMessageW(
                    Some(window),
                    CAPTURE_MESSAGE,
                    WPARAM(start as usize),
                    LPARAM(0),
                )?;
            }
            if unsafe { WaitForSingleObject(worker.handle(), 5000) } != WAIT_OBJECT_0 {
                return Err(Error::new(
                    E_FAIL,
                    "Prepared editor did not paint and close",
                ));
            }
            if worker.exit_code()? != Some(0) {
                return Err(Error::new(E_FAIL, "Prepared editor failed"));
            }
            let elapsed = std::fs::read_to_string(REPORT)
                .map_err(io)?
                .trim()
                .parse::<f64>()
                .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
            samples.push(elapsed);
            // The preceding process is dead before its replacement can be constructed.
        }
        reports.push(format!("\"{}\":{{\"request_to_paint_gdi_flush_ms\":{:?},\"standby_working_set_bytes\":{},\"standby_private_bytes\":{},\"standby_cpu_ms_over_2s\":{}}}", if live { "live" } else { "frozen" }, samples, working_set, private_bytes, idle_cpu_ms));
    }
    std::fs::write(
        "target/warm-editor-diagnostics.json",
        format!("{{{}}}\n", reports.join(",")),
    )
    .map_err(io)
}
