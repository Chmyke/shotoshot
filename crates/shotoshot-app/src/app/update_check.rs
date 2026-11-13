//! ROLE: "Check for updates" on Windows: on the Settings button's click only, one HTTPS request
//! to the site, off the resident thread, then the answer in the Settings window; a newer
//! version's button opens the download page in the browser.
//! OWNER: `SettingsUi` holds one `UpdateCheck`; it outlives the Settings window, so a window
//! opened again shows what the last check found.
//! INVARIANTS: Nothing reaches the network before the click (law 4). The app never downloads
//! nor runs an installer (Mickael, 2026-11-13, A1). One check at a time. A completion that
//! arrives while App is borrowed is retried by timer, never lost: the row would stay
//! "Checking" forever.
use super::*;
use shotoshot_core::update::{self, DOWNLOAD_PAGE, LATEST_HOST, LATEST_PATH, MAX_ANSWER, Outcome};
use shotoshot_ui::settings_panel::{UpdateState, running_version};
use std::sync::mpsc;
use windows::Win32::Networking::WinHttp::*;

/// A check finished: the resident reads its answer.
pub(super) const CHECKED: u32 = WM_APP + 19;
pub(super) const RETRY_TIMER: usize = 0x5555;
/// Each step of the request gives up after this long: the row never waits much longer.
const TIMEOUT_MS: i32 = 10_000;

#[derive(Default)]
pub(super) struct UpdateCheck {
    pub(super) state: UpdateState,
    pending: Option<mpsc::Receiver<Option<Outcome>>>,
}

impl App {
    /// Starts the one request of "Check for updates"; a check already running is left to end.
    pub(super) fn check_for_updates(&mut self, owner: HWND) {
        if self.settings_ui.update.pending.is_some() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.settings_ui.update.pending = Some(receiver);
        self.settings_ui.update.state = UpdateState::Checking;
        // HWND is not Send: the thread gets its value back.
        let owner = owner.0 as isize;
        std::thread::spawn(move || {
            let outcome = fetch_latest()
                .ok()
                .and_then(|body| update::check(running_version(), &body));
            let _ = sender.send(outcome);
            unsafe {
                let _ = PostMessageW(Some(HWND(owner as *mut _)), CHECKED, WPARAM(0), LPARAM(0));
            }
        });
    }
    /// Takes the answer of the check, if it came, and shows it in the open Settings window.
    pub(super) fn finish_update_check(&mut self) -> Result<()> {
        let Some(receiver) = &self.settings_ui.update.pending else {
            return Ok(());
        };
        let outcome = match receiver.try_recv() {
            Ok(outcome) => outcome,
            Err(mpsc::TryRecvError::Empty) => return Ok(()),
            Err(mpsc::TryRecvError::Disconnected) => None,
        };
        self.settings_ui.update.pending = None;
        self.settings_ui.update.state = match outcome {
            Some(Outcome::UpToDate) => UpdateState::UpToDate,
            Some(Outcome::Available(version)) => UpdateState::Available(version),
            None => UpdateState::Failed,
        };
        self.refresh_settings_window()
    }
}

/// Opens the download page in the default browser, which downloads as it always did.
pub(super) fn open_download_page(owner: HWND) -> Result<()> {
    let opened = unsafe {
        ShellExecuteW(
            Some(owner),
            w!("open"),
            &HSTRING::from(DOWNLOAD_PAGE),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    if opened.0 as isize <= 32 {
        return Err(Error::from_thread());
    }
    Ok(())
}

/// A WinHTTP handle, closed when dropped.
struct Handle(*mut core::ffi::c_void);
impl Handle {
    fn new(raw: *mut core::ffi::c_void) -> Result<Self> {
        if raw.is_null() {
            return Err(Error::from_thread());
        }
        Ok(Self(raw))
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = WinHttpCloseHandle(self.0);
        }
    }
}

/// The site's answer to `GET https://shotoshot.com/api/shotoshot/latest`, the system's proxy
/// followed, no redirect followed, at most `MAX_ANSWER` bytes; any other status is a failure.
fn fetch_latest() -> Result<String> {
    unsafe {
        let session = Handle::new(WinHttpOpen(
            w!("Shotoshot"),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        ))?;
        WinHttpSetTimeouts(session.0, TIMEOUT_MS, TIMEOUT_MS, TIMEOUT_MS, TIMEOUT_MS)?;
        let connection = Handle::new(WinHttpConnect(
            session.0,
            &HSTRING::from(LATEST_HOST),
            INTERNET_DEFAULT_HTTPS_PORT,
            0,
        ))?;
        let request = Handle::new(WinHttpOpenRequest(
            connection.0,
            w!("GET"),
            &HSTRING::from(LATEST_PATH),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ))?;
        // The answer comes from shotoshot.com itself or not at all.
        let never = WINHTTP_OPTION_REDIRECT_POLICY_NEVER.to_ne_bytes();
        WinHttpSetOption(
            Some(request.0),
            WINHTTP_OPTION_REDIRECT_POLICY,
            Some(&never),
        )?;
        WinHttpSendRequest(request.0, None, None, 0, 0, 0)?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut())?;
        let mut status = 0u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&mut status as *mut u32).cast()),
            &mut size,
            std::ptr::null_mut(),
        )?;
        if status != 200 {
            return Err(Error::new(E_FAIL, format!("HTTP {status}")));
        }
        let mut body = Vec::new();
        let mut chunk = [0u8; 512];
        loop {
            let mut read = 0u32;
            WinHttpReadData(
                request.0,
                chunk.as_mut_ptr().cast(),
                chunk.len() as u32,
                &mut read,
            )?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..read as usize]);
            if body.len() > MAX_ANSWER {
                return Err(Error::new(E_FAIL, "answer too long"));
            }
        }
        String::from_utf8(body).map_err(|_| Error::new(E_FAIL, "answer not UTF-8"))
    }
}

#[cfg(test)]
mod tests {
    /// Asks the real site: never part of `cargo test`, run by hand after a change of the request.
    #[test]
    #[ignore = "asks shotoshot.com over the network"]
    fn real_network_the_site_answers_with_a_version() {
        let body = super::fetch_latest().unwrap();
        assert!(
            shotoshot_core::update::published(&body).is_some(),
            "{body:?}"
        );
    }
}
