//! On-demand capture and PNG encoding off the resident thread; no focus change or idle polling.
use super::*;
use std::{path::Path, sync::mpsc};

pub(super) const FINISHED: u32 = WM_APP + 6;
pub(super) const RETRY_TIMER: usize = 0x5353;
struct Pending {
    receiver: mpsc::Receiver<std::result::Result<(), String>>,
    bounds: RECT,
    sound: bool,
}
pub(super) struct InstantCapture {
    pub hotkey: crate::hotkey::Hotkey,
    pending: Option<Pending>,
    pulse: Option<pulse::Pulse>,
}
impl InstantCapture {
    pub fn new(hwnd: HWND) -> Self {
        Self {
            hotkey: crate::hotkey::Hotkey::with_id(hwnd, 3),
            pending: None,
            pulse: None,
        }
    }
    pub fn clear_feedback(&mut self) {
        self.pulse = None;
    }
    fn start(&mut self, hwnd: HWND, sound: bool) -> Result<()> {
        if self.pending.is_some() {
            return Ok(());
        }
        let monitor = unsafe { MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTOPRIMARY) };
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        unsafe {
            GetMonitorInfoW(monitor, &mut info).ok()?;
        }
        let (sender, receiver) = mpsc::channel();
        let owner = hwnd.0 as usize;
        let monitor = monitor.0 as usize;
        std::thread::Builder::new()
            .name("instant-screenshot".into())
            .spawn(move || {
                let outcome = std::panic::catch_unwind(|| -> Result<()> {
                    let _com = ComApartment::new()?;
                    let image = crate::game_capture::capture_monitor(HMONITOR(monitor as *mut _))?;
                    let desktop = crate::settings::known_folder(&FOLDERID_Desktop)?;
                    save_to(&image, &desktop)
                })
                .map_err(|_| "Screenshot worker stopped unexpectedly".to_string())
                .and_then(|r| r.map_err(|e| e.to_string()));
                let _ = sender.send(outcome);
                unsafe {
                    let _ =
                        PostMessageW(Some(HWND(owner as *mut _)), FINISHED, WPARAM(0), LPARAM(0));
                }
            })
            .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
        self.pending = Some(Pending {
            receiver,
            bounds: info.rcMonitor,
            sound,
        });
        Ok(())
    }
}

fn save_to(image: &Image, folder: &Path) -> Result<()> {
    // No dialog, so no format choice: the instant capture stays lossless.
    let format = crate::settings::SaveFormat::Png;
    let path = crate::capture_name::next_path(folder, format)
        .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
    crate::save_numbered(image, &path, format)?;
    Ok(())
}

impl App {
    pub(super) fn start_instant_capture(&mut self, hwnd: HWND) {
        if self.setup || self.session.is_some() || self.worker_busy() {
            return;
        }
        self.toast = None;
        self.instant.clear_feedback();
        if let Err(error) = self.instant.start(hwnd, self.settings.instant_sound) {
            self.toast =
                notice::Toast::new(&format!("Screenshot failed: {}", error.message()), true).ok();
        }
    }
    pub(super) fn finish_instant_capture(&mut self) {
        let Some(pending) = self.instant.pending.as_ref() else {
            return;
        };
        let result = match pending.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err("Screenshot interrupted".into()),
        };
        let pending = self.instant.pending.take().unwrap();
        let success = result.is_ok();
        // Success is confirmed by the border pulse and sound only; text is reserved for failures.
        self.toast = if success {
            None
        } else {
            notice::Toast::new_at(
                "Screenshot failed — file not saved",
                self.persist_preferences,
                Some(pending.bounds),
            )
            .ok()
        };
        self.instant.pulse = if success {
            pulse::Pulse::new(pending.bounds, self.persist_preferences).ok()
        } else {
            None
        };
        if success && pending.sound && self.persist_preferences {
            play_sound();
        }
        if let Err(error) = result {
            eprintln!("Instant screenshot: {error}");
        }
    }
    pub(super) fn save_instant_shortcut(&mut self, hwnd: HWND, mods: u32, key: u32) -> Result<()> {
        let shortcut = crate::settings::Shortcut {
            modifiers: mods,
            key,
        };
        if (self.settings.modifiers == mods && self.settings.key == key)
            || self
                .settings
                .presentation
                .shape_shortcuts
                .contains(&shortcut)
            || (mods == 3 && key == 27)
        {
            self.setup_ui.error = "This shortcut is already used by Shotoshot.".into();
            return Ok(());
        }
        if self.settings.board_shortcut == shortcut {
            self.setup_ui.error = "This shortcut is already used by the board.".into();
            return Ok(());
        }
        if !(self.instant.hotkey.active() && self.settings.instant_shortcut == shortcut) {
            let mut next = self.settings.clone();
            next.instant_shortcut = shortcut;
            let free = crate::print_screen::windows_option(self.persist_preferences);
            if let Err(error) = self.instant.hotkey.replace(free, mods, key, || {
                if self.persist_preferences {
                    next.save().map_err(|e| Error::new(E_FAIL, e))
                } else {
                    Ok(())
                }
            }) {
                self.setup_ui.error = super::setup::print_screen_error(mods, key, &error)
                    .unwrap_or_else(|| {
                        format!(
                            "Shortcut unavailable ({}). Choose another key.",
                            error.code().0
                        )
                    });
                return Ok(());
            }
            self.settings = next;
            self.restore_print_screen_if_unused();
        }
        self.close_setup(hwnd)?;
        self.notify(Notice::Preferences);
        Ok(())
    }
}

fn play_sound() {
    use windows::Win32::Media::Audio::*;
    // Optional user-owned override stays outside the application and its installer.
    if let Ok(folder) = crate::settings::known_folder(&FOLDERID_LocalAppData) {
        let path = folder.join("Shotoshot").join("screenshot-custom.wav");
        if path.is_file()
            && unsafe {
                PlaySoundW(
                    PCWSTR(crate::wide(&path).as_ptr()),
                    None,
                    SND_FILENAME | SND_ASYNC | SND_NODEFAULT,
                )
                .as_bool()
            }
        {
            return;
        }
    }
    // Original short shutter sound; static bytes outlive asynchronous playback.
    static SOUND: &[u8] = include_bytes!("../../assets/screenshot.wav");
    unsafe {
        let _ = PlaySoundW(
            PCWSTR(SOUND.as_ptr().cast()),
            None,
            SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_app(hwnd: HWND) -> App {
        App {
            settings: Settings::default(),
            session: None,
            setup: false,
            setup_ui: setup::Setup::default(),
            hotkey: crate::hotkey::Hotkey::new(hwnd),
            instant: InstantCapture::new(hwnd),
            board_hotkey: crate::hotkey::Hotkey::with_id(hwnd, 5),
            tray: NOTIFYICONDATAW::default(),
            quit_after_capture: false,
            persist_preferences: false,
            pending_brush: None,
            worker: None,
            prepared: None,
            notification_owner: None,
            toast: None,
            google_search: None,
            first_frame: None,
            presentation: None,
            board_rebake_forced: false,
            menu: None,
            menu_closed_on_icon: None,
        }
    }
    #[test]
    fn confirmation_waits_for_success_and_configuration_rejects_other_shortcuts() {
        let owner = crate::resources::OwnedWindow(unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                w!("STATIC"),
                w!("Instant test"),
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
            .unwrap()
        });
        let mut app = test_app(owner.0);
        let (sender, receiver) = mpsc::channel();
        app.instant.pending = Some(Pending {
            receiver,
            bounds: RECT {
                right: 1920,
                bottom: 1080,
                ..Default::default()
            },
            sound: false,
        });
        app.finish_instant_capture();
        assert!(app.toast.is_none());
        assert!(app.instant.pending.is_some());
        sender.send(Ok(())).unwrap();
        app.finish_instant_capture();
        assert!(app.instant.pending.is_none());
        assert!(app.toast.is_none(), "success is confirmed without text");
        assert!(app.instant.pulse.is_some());
        assert!(crate::app::nothing_activated_on_this_thread());
        let original = app.settings.instant_shortcut;
        app.settings.key = 19;
        app.save_instant_shortcut(owner.0, 0, 19).unwrap();
        assert!(!app.setup_ui.error.is_empty());
        assert_eq!(app.settings.instant_shortcut, original);
        app.setup_ui.error.clear();
        app.save_instant_shortcut(owner.0, 1, 0x52).unwrap();
        assert!(!app.setup_ui.error.is_empty());
        assert_eq!(app.settings.instant_shortcut, original);
    }
    #[test]
    fn instant_export_is_numbered_and_does_not_modify_previous_files() {
        let _com = ComApartment::new().unwrap();
        let root = std::env::temp_dir().join(format!("shotoshot-instant-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("Capture 001.png"), b"old").unwrap();
        let image = Image::solid(3, 2, [30, 90, 180, 255]);
        save_to(&image, &root).unwrap();
        assert_eq!(
            crate::load_png(&root.join("Shot 002.png")).unwrap().pixels,
            image.pixels
        );
        assert_eq!(std::fs::read(root.join("Capture 001.png")).unwrap(), b"old");
        assert!(save_to(&image, &root.join("missing")).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn completion_during_modal_borrow_is_deferred_and_failure_unlocks_next_capture() {
        unsafe {
            let owner = crate::resources::OwnedWindow(
                CreateWindowExW(
                    WS_EX_TOOLWINDOW,
                    w!("STATIC"),
                    w!("Deferred completion test"),
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
                .unwrap(),
            );
            let state = RefCell::new(test_app(owner.0));
            let (sender, receiver) = mpsc::channel();
            state.borrow_mut().instant.pending = Some(Pending {
                receiver,
                bounds: RECT {
                    right: 1920,
                    bottom: 1080,
                    ..Default::default()
                },
                sound: true,
            });
            sender.send(Err("Disk full".into())).unwrap();
            SetWindowLongPtrW(
                owner.0,
                GWLP_USERDATA,
                &state as *const RefCell<App> as isize,
            );
            let borrowed = state.borrow_mut();
            window_proc(owner.0, FINISHED, WPARAM(0), LPARAM(0));
            assert!(borrowed.instant.pending.is_some());
            drop(borrowed);
            assert!(
                KillTimer(Some(owner.0), 0x5353).is_ok(),
                "completion must schedule a deferred delivery"
            );
            window_proc(owner.0, WM_TIMER, WPARAM(0x5353), LPARAM(0));
            SetWindowLongPtrW(owner.0, GWLP_USERDATA, 0);
            let mut app = state.borrow_mut();
            assert!(app.instant.pending.is_none());
            assert!(app.instant.pulse.is_none());
            assert_eq!(
                app.toast.as_ref().unwrap().text(),
                "Screenshot failed — file not saved"
            );
            let (sender, receiver) = mpsc::channel();
            app.instant.pending = Some(Pending {
                receiver,
                bounds: RECT {
                    right: 1920,
                    bottom: 1080,
                    ..Default::default()
                },
                sound: true,
            });
            drop(sender);
            app.finish_instant_capture();
            assert!(app.instant.pending.is_none());
            assert!(app.instant.pulse.is_none());
            assert_eq!(
                app.toast.as_ref().unwrap().text(),
                "Screenshot failed — file not saved"
            );
        }
    }
}
