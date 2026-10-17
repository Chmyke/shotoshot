//! One prepared editor owned by the resident. No polling and no desktop pixels cross IPC.
use std::{os::windows::ffi::OsStrExt, path::Path};
use windows::{
    Win32::{
        Foundation::*,
        System::{JobObjects::*, Threading::*},
        UI::WindowsAndMessaging::*,
    },
    core::*,
};

struct Handle(HANDLE);
// SAFETY: these are kernel process/job handles; waiting and closing have no thread affinity.
unsafe impl Send for Handle {}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
struct JobChild {
    _job: Handle,
    process: Handle,
    pid: u32,
}
impl JobChild {
    fn launch(executable: &Path, arguments: &str) -> Result<Self> {
        unsafe {
            let job = Handle(CreateJobObjectW(None, PCWSTR::null())?);
            let limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
                BasicLimitInformation: JOBOBJECT_BASIC_LIMIT_INFORMATION {
                    // Lens may start a browser: it must survive the editor/resident.
                    LimitFlags: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
                        | JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK,
                    ..Default::default()
                },
                ..Default::default()
            };
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limits) as u32,
            )?;
            // Assign at creation, closing the crash window between spawn and job assignment.
            // Only the resident owns this handle: inheriting it would defeat kill-on-close.
            let attributes = Attributes::new()?;
            UpdateProcThreadAttribute(
                attributes.list,
                0,
                PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
                Some((&job.0 as *const HANDLE).cast()),
                std::mem::size_of::<HANDLE>(),
                None,
                None,
            )?;
            let startup = STARTUPINFOEXW {
                StartupInfo: STARTUPINFOW {
                    cb: std::mem::size_of::<STARTUPINFOEXW>() as u32,
                    ..Default::default()
                },
                lpAttributeList: attributes.list,
            };
            let path: Vec<u16> = executable
                .as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect();
            // Windows file names cannot contain quotes. Arguments are internal switches/numbers.
            let mut command: Vec<u16> = [u16::from(b'"')]
                .into_iter()
                .chain(path[..path.len() - 1].iter().copied())
                .chain([u16::from(b'"'), u16::from(b' ')])
                .chain(arguments.encode_utf16())
                .chain(Some(0))
                .collect();
            let mut info = PROCESS_INFORMATION::default();
            CreateProcessW(
                PCWSTR(path.as_ptr()),
                Some(PWSTR(command.as_mut_ptr())),
                None,
                None,
                false,
                CREATE_NO_WINDOW | EXTENDED_STARTUPINFO_PRESENT,
                None,
                PCWSTR::null(),
                &startup.StartupInfo,
                &mut info,
            )?;
            let _thread = Handle(info.hThread);
            Ok(Self {
                _job: job,
                process: Handle(info.hProcess),
                pid: info.dwProcessId,
            })
        }
    }
    fn watch(&self, owner: HWND) -> Result<()> {
        let mut duplicate = HANDLE::default();
        unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                self.process.0,
                GetCurrentProcess(),
                &mut duplicate,
                0,
                false,
                DUPLICATE_SAME_ACCESS,
            )?;
        }
        let observer = Handle(duplicate);
        let (owner, pid) = (owner.0 as usize, self.pid);
        std::thread::Builder::new()
            .name("Shotoshot editor exit".into())
            .spawn(move || wait_and_post(observer, owner, pid))
            .map(|_| ())
            .map_err(|e| Error::new(E_FAIL, e.to_string()))
    }
}
fn wait_and_post(process: Handle, owner: usize, pid: u32) {
    unsafe {
        if WaitForSingleObject(process.0, INFINITE) == WAIT_OBJECT_0 {
            let mut exit_code = 1;
            let _ = GetExitCodeProcess(process.0, &mut exit_code);
            let _ = PostMessageW(
                Some(HWND(owner as _)),
                EXITED,
                WPARAM(pid as usize),
                LPARAM(exit_code as isize),
            );
        }
    }
}

pub(super) const READY: u32 = WM_APP + 8;
pub(super) const EXITED: u32 = WM_APP + 9;

/// The resident keeps one Option<EditorProcess>, replacing it only after its process exits.
pub(super) struct EditorProcess {
    child: JobChild,
    activation: Activation,
    prepared: bool,
}
impl EditorProcess {
    pub fn spawn(owner: HWND, prepared: bool) -> Result<Self> {
        Self::launch(owner, prepared, "")
    }
    pub fn launch(owner: HWND, prepared: bool, diagnostic: &str) -> Result<Self> {
        let executable = std::env::current_exe().map_err(|e| Error::new(E_FAIL, e.to_string()))?;
        let child = JobChild::launch(
            &executable,
            &format!(
                "--prepared-editor --notification-owner={} {} {diagnostic}",
                owner.0 as usize,
                if prepared { "" } else { "--cold-editor" }
            ),
        )?;
        child.watch(owner)?;
        Ok(Self {
            child,
            activation: Activation::default(),
            prepared,
        })
    }
    pub fn pid(&self) -> u32 {
        self.child.pid
    }
    pub fn handle(&self) -> HANDLE {
        self.child.process.0
    }
    pub fn request(&mut self) -> Option<HWND> {
        self.activation.request()
    }
    pub fn ready(&mut self, window: HWND) -> Option<HWND> {
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(window, Some(&mut pid));
        }
        if pid != self.pid() || unsafe { WaitForSingleObject(self.handle(), 0) } != WAIT_TIMEOUT {
            return None;
        }
        self.activation.ready(window)
    }
    pub fn busy(&self) -> bool {
        self.activation.busy()
    }
    /// The window of the capture still open, possibly behind other applications.
    pub fn active_window(&self) -> Option<HWND> {
        self.activation.active_window()
    }
    pub fn replenish(&self, exit_code: u32) -> bool {
        self.activation.replenish(exit_code)
    }
    pub fn retry_cold(&self) -> bool {
        self.prepared && self.activation.pending
    }
    pub fn pending_request(&self) -> bool {
        self.activation.pending
    }
    pub fn stop(&self) {
        unsafe {
            let _ = TerminateJobObject(self.child._job.0, 1);
        }
    }
    /// Give an active editor time to flush pending preferences before closing its job.
    /// A hung/modal editor is still bounded; a resident crash simply closes the job.
    pub fn shutdown(self) {
        if let Phase::Ready(window) | Phase::Active(window) = self.activation.phase {
            unsafe {
                let _ = PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0));
                WaitForSingleObject(self.handle(), 3000);
            }
        }
        self.stop();
        unsafe {
            WaitForSingleObject(self.handle(), 1500);
        }
    }
    pub fn exit_code(&self) -> Result<Option<u32>> {
        unsafe {
            if WaitForSingleObject(self.handle(), 0) != WAIT_OBJECT_0 {
                return Ok(None);
            }
            let mut code = 0;
            GetExitCodeProcess(self.handle(), &mut code)?;
            Ok(Some(code))
        }
    }
}

struct Attributes {
    _storage: Vec<usize>,
    list: LPPROC_THREAD_ATTRIBUTE_LIST,
}
impl Attributes {
    fn new() -> Result<Self> {
        unsafe {
            let mut bytes = 0;
            let _ = InitializeProcThreadAttributeList(None, 1, None, &mut bytes);
            if bytes == 0 {
                return Err(Error::from_thread());
            }
            let mut storage = vec![0usize; bytes.div_ceil(std::mem::size_of::<usize>())];
            let list = LPPROC_THREAD_ATTRIBUTE_LIST(storage.as_mut_ptr().cast());
            InitializeProcThreadAttributeList(Some(list), 1, None, &mut bytes)?;
            Ok(Self {
                _storage: storage,
                list,
            })
        }
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        unsafe { DeleteProcThreadAttributeList(self.list) }
    }
}

#[derive(Default)]
enum Phase {
    #[default]
    Preparing,
    Ready(HWND),
    Active(HWND),
}

#[derive(Default)]
struct Activation {
    phase: Phase,
    pending: bool,
}
impl Activation {
    fn request(&mut self) -> Option<HWND> {
        match self.phase {
            Phase::Preparing => {
                self.pending = true;
                None
            }
            Phase::Ready(window) => {
                self.phase = Phase::Active(window);
                Some(window)
            }
            Phase::Active(_) => None,
        }
    }
    fn ready(&mut self, window: HWND) -> Option<HWND> {
        if !matches!(self.phase, Phase::Preparing) {
            return None;
        }
        self.phase = Phase::Ready(window);
        if std::mem::take(&mut self.pending) {
            self.request()
        } else {
            None
        }
    }
    fn busy(&self) -> bool {
        self.pending || matches!(self.phase, Phase::Active(_))
    }
    fn active_window(&self) -> Option<HWND> {
        match self.phase {
            Phase::Active(window) => Some(window),
            _ => None,
        }
    }
    fn replenish(&self, exit_code: u32) -> bool {
        exit_code == 0 && matches!(self.phase, Phase::Active(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn fixture_arguments(name: &str) -> String {
        let scope = module_path!().split_once("::").unwrap().1;
        format!("--ignored --exact {scope}::{name} --test-threads=1")
    }
    fn child_fixture(name: &str) -> JobChild {
        JobChild::launch(&std::env::current_exe().unwrap(), &fixture_arguments(name))
            .expect("launch the isolated, noninteractive test child")
    }

    #[test]
    #[ignore = "isolated child fixture; started only by the job tests"]
    fn waiting_child() {
        std::thread::sleep(Duration::from_secs(10));
    }

    #[test]
    fn dropping_the_owner_closes_its_child() {
        let child = child_fixture("waiting_child");
        let observer = Handle(
            unsafe {
                OpenProcess(
                    PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
                    false,
                    child.pid,
                )
            }
            .unwrap(),
        );
        assert_eq!(
            unsafe { WaitForSingleObject(observer.0, 100) },
            WAIT_TIMEOUT
        );
        drop(child);
        let exited = unsafe { WaitForSingleObject(observer.0, 3000) };
        if exited != WAIT_OBJECT_0 {
            let cleanup =
                unsafe { OpenProcess(PROCESS_TERMINATE, false, GetProcessId(observer.0)) };
            if let Ok(process) = cleanup {
                let process = Handle(process);
                let _ = unsafe { TerminateProcess(process.0, 1) };
            }
        }
        assert_eq!(exited, WAIT_OBJECT_0, "the child outlived its owner");
    }

    fn pid_file(pid: u32) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("shotoshot-editor-owner-{pid}.txt"))
    }
    fn published_child(owner: &JobChild) -> Handle {
        let path = pid_file(owner.pid);
        let until = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            if let Ok(text) = std::fs::read_to_string(&path)
                && let Ok(pid) = text.parse::<u32>()
            {
                std::fs::remove_file(path).unwrap();
                return Handle(
                    unsafe { OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_TERMINATE, false, pid) }
                        .unwrap(),
                );
            }
            assert!(std::time::Instant::now() < until, "child PID not published");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    #[ignore = "isolated owner fixture; started only by the job tests"]
    fn owner_child() {
        let child = child_fixture("waiting_child");
        std::fs::write(pid_file(std::process::id()), child.pid.to_string()).unwrap();
        std::thread::sleep(Duration::from_secs(10));
    }

    #[test]
    fn a_crashed_owner_leaves_no_orphan() {
        let owner = child_fixture("owner_child");
        let child = published_child(&owner);
        unsafe { TerminateProcess(owner.process.0, 17) }.unwrap();
        let exited = unsafe { WaitForSingleObject(child.0, 3000) };
        let _ = unsafe { TerminateProcess(child.0, 1) };
        assert_eq!(exited, WAIT_OBJECT_0, "crash left the editor running");
    }

    #[test]
    #[ignore = "isolated external-launch fixture; started only by the job tests"]
    fn external_launcher_child() {
        use std::os::windows::process::CommandExt;
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(fixture_arguments("waiting_child").split_whitespace())
            .creation_flags(CREATE_NO_WINDOW.0)
            .spawn()
            .unwrap();
        std::fs::write(pid_file(std::process::id()), child.id().to_string()).unwrap();
        std::thread::sleep(Duration::from_secs(10));
        let _ = child.wait();
    }

    #[test]
    fn an_explicit_external_program_outlives_the_editor_job() {
        let editor = child_fixture("external_launcher_child");
        let external = published_child(&editor);
        drop(editor);
        let exited = unsafe { WaitForSingleObject(external.0, 200) };
        let _ = unsafe { TerminateProcess(external.0, 1) };
        assert_eq!(
            exited, WAIT_TIMEOUT,
            "the external program inherited the editor job"
        );
    }

    #[test]
    fn an_unrelated_window_cannot_trigger_the_pending_capture() {
        let child = child_fixture("waiting_child");
        let mut editor = EditorProcess {
            child,
            activation: Activation::default(),
            prepared: true,
        };
        assert_eq!(editor.request(), None);
        assert_eq!(editor.ready(HWND::default()), None);
        assert!(editor.busy());
        assert!(
            !editor.replenish(0),
            "an invalid READY marked the capture active"
        );
    }

    #[test]
    fn process_exit_posts_one_completion_without_polling() {
        unsafe {
            let window = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                w!("STATIC"),
                w!(""),
                WS_POPUP,
                -32000,
                -32000,
                1,
                1,
                None,
                None,
                None,
                None,
            )
            .unwrap();
            let child = child_fixture("waiting_child");
            child.watch(window).unwrap();
            TerminateProcess(child.process.0, 23).unwrap();
            assert_eq!(WaitForSingleObject(child.process.0, 3000), WAIT_OBJECT_0);
            let until = std::time::Instant::now() + Duration::from_secs(3);
            let mut message = MSG::default();
            let received = loop {
                if PeekMessageW(&mut message, Some(window), EXITED, EXITED, PM_REMOVE).as_bool() {
                    break true;
                }
                if std::time::Instant::now() >= until {
                    break false;
                }
                std::thread::sleep(Duration::from_millis(5));
            };
            let duplicate =
                PeekMessageW(&mut MSG::default(), Some(window), EXITED, EXITED, PM_REMOVE)
                    .as_bool();
            DestroyWindow(window).unwrap();
            assert!(received, "the owner was never notified of the exit");
            assert_eq!(message.wParam.0, child.pid as usize);
            assert_eq!(message.lParam.0, 23);
            assert!(!duplicate);
        }
    }

    #[test]
    fn requests_before_readiness_open_exactly_one_capture() {
        let mut activation = Activation::default();
        assert!(!activation.busy());
        for _ in 0..20 {
            assert_eq!(activation.request(), None);
        }
        assert!(activation.busy(), "the first request must remain pending");
        let window = HWND(17 as _);
        assert_eq!(activation.ready(window), Some(window));
        assert_eq!(activation.ready(window), None);
        assert_eq!(activation.request(), None);
    }

    #[test]
    fn readiness_never_opens_the_editor_without_a_request() {
        let mut activation = Activation::default();
        let window = HWND(17 as _);
        assert_eq!(activation.ready(window), None);
        assert!(!activation.busy());
        assert_eq!(activation.request(), Some(window));
        assert!(activation.busy());
        assert_eq!(activation.request(), None);
    }

    #[test]
    fn only_an_open_capture_can_be_brought_back() {
        let mut activation = Activation::default();
        assert_eq!(activation.active_window(), None);
        assert_eq!(activation.request(), None);
        assert_eq!(activation.active_window(), None, "still preparing");
        let window = HWND(17 as _);
        assert_eq!(activation.ready(window), Some(window));
        assert_eq!(activation.active_window(), Some(window));
        assert_eq!(activation.request(), None, "a second capture never starts");
        assert_eq!(activation.active_window(), Some(window));
        let mut idle = Activation::default();
        assert_eq!(idle.ready(window), None);
        assert_eq!(
            idle.active_window(),
            None,
            "a prepared editor shows nothing"
        );
    }

    #[test]
    fn only_a_successfully_finished_capture_is_prepared_again() {
        let mut activation = Activation::default();
        assert!(!activation.replenish(0));
        assert!(!activation.replenish(1));
        activation.ready(HWND(17 as _));
        assert!(!activation.replenish(0));
        activation.request();
        assert!(activation.replenish(0));
        assert!(!activation.replenish(1));
    }

    #[test]
    fn preparation_failure_retries_only_a_pending_warm_request() {
        for prepared in [true, false] {
            let mut editor = EditorProcess {
                child: child_fixture("waiting_child"),
                activation: Activation::default(),
                prepared,
            };
            assert!(!editor.pending_request());
            assert!(
                !editor.retry_cold(),
                "an idle failure must never restart itself"
            );
            editor.request();
            assert!(editor.pending_request());
            assert_eq!(
                editor.retry_cold(),
                prepared,
                "cold failures must be reported, not retried forever"
            );
            assert!(!editor.replenish(0));
            assert!(!editor.replenish(1));
        }
    }

    #[test]
    #[ignore = "isolated graceful-shutdown fixture"]
    fn closing_child() {
        unsafe {
            let window = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                w!("STATIC"),
                w!(""),
                WS_POPUP,
                -32000,
                -32000,
                1,
                1,
                None,
                None,
                None,
                None,
            )
            .unwrap();
            std::fs::write(
                pid_file(std::process::id()),
                (window.0 as usize).to_string(),
            )
            .unwrap();
            let mut message = MSG::default();
            while GetMessageW(&mut message, None, 0, 0).0 > 0 {
                if message.message == WM_CLOSE {
                    // Models the editor's synchronous close/flush before its process exits.
                    std::fs::write(pid_file(std::process::id()), "flushed").unwrap();
                    break;
                }
                DispatchMessageW(&message);
            }
            DestroyWindow(window).unwrap();
        }
    }

    #[test]
    fn normal_shutdown_waits_for_the_child_close_handler() {
        let child = child_fixture("closing_child");
        let path = pid_file(child.pid);
        let until = std::time::Instant::now() + Duration::from_secs(3);
        let window = loop {
            if let Ok(text) = std::fs::read_to_string(&path)
                && let Ok(window) = text.parse::<usize>()
            {
                break HWND(window as _);
            }
            assert!(
                std::time::Instant::now() < until,
                "fixture did not publish its window"
            );
            std::thread::sleep(Duration::from_millis(5));
        };
        let mut editor = EditorProcess {
            child,
            activation: Activation::default(),
            prepared: true,
        };
        editor.ready(window);
        assert_eq!(editor.request(), Some(window));
        editor.shutdown();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "flushed",
            "job kill interrupted normal cleanup"
        );
        std::fs::remove_file(path).unwrap();
    }
}
