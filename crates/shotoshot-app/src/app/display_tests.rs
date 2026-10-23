//! Native display notifications must preserve presentation choices, including during a menu/capture.
use super::*;
use crate::presentation::Enabled;

struct Resident {
    window: crate::resources::OwnedWindow,
    state: Box<RefCell<App>>,
    _com: ComApartment,
}
impl Resident {
    fn new() -> Self {
        let com = ComApartment::new().unwrap();
        unsafe {
            let instance = GetModuleHandleW(None).unwrap();
            if RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance.into(),
                lpszClassName: w!("Shotoshot.DisplayTest"),
                ..Default::default()
            }) == 0
            {
                assert_eq!(GetLastError(), ERROR_CLASS_ALREADY_EXISTS);
            }
            let state = Box::new(RefCell::new(App {
                settings: Settings::default(),
                session: None,
                setup: false,
                hotkey: crate::hotkey::Hotkey::new(HWND::default()),
                instant: instant::InstantCapture::new(HWND::default()),
                board_hotkey: crate::hotkey::Hotkey::with_id(HWND::default(), 5),
                tray: NOTIFYICONDATAW::default(),
                quit_after_capture: false,
                persist_preferences: false,
                pending_brush: None,
                worker: None,
                prepared: None,
                recall: Default::default(),
                notification_owner: None,
                google_search: None,
                first_frame: None,
                toast: None,
                presentation: None,
                board_rebake_forced: false,
                menu: None,
                menu_closed_on_icon: None,
                setup_ui: setup::Setup::default(),
            }));
            let window = crate::resources::OwnedWindow(
                CreateWindowExW(
                    WS_EX_TOOLWINDOW,
                    w!("Shotoshot.DisplayTest"),
                    w!("Display test"),
                    WS_POPUP,
                    -32000,
                    -32000,
                    1,
                    1,
                    None,
                    None,
                    Some(instance.into()),
                    Some((&*state as *const RefCell<App>).cast()),
                )
                .unwrap(),
            );
            state.borrow_mut().presentation =
                Some(crate::presentation::Presentation::new(window.0, Default::default()).unwrap());
            Self {
                window,
                state,
                _com: com,
            }
        }
    }
    fn presentation(&self, action: impl FnOnce(&mut crate::presentation::Presentation)) {
        action(
            &mut self
                .state
                .borrow()
                .presentation
                .as_ref()
                .unwrap()
                .borrow_mut(),
        );
    }
    fn send(&self, msg: u32, w: usize) {
        unsafe {
            SendMessageW(self.window.0, msg, Some(WPARAM(w)), Some(LPARAM(0)));
        }
    }
    fn display_change(&self) {
        self.send(WM_DISPLAYCHANGE, 32);
    }
}

#[test]
fn display_change_preserves_each_feature_choice_and_preferences() {
    let resident = Resident::new();
    for mask in 0..16 {
        let enabled = Enabled {
            halo: mask & 1 != 0,
            keys: mask & 2 != 0,
            shapes: mask & 4 != 0,
            mouse: mask & 8 != 0,
        };
        resident.presentation(|p| {
            p.preferences.input_offset = Some([2300, 6700]);
            p.preferences.input_locked = true;
            p.set_enabled(enabled).unwrap();
        });
        for _ in 0..2 {
            resident.display_change();
            resident.presentation(|p| {
                assert!(
                    p.enabled == enabled,
                    "display change lost active features, mask {mask}"
                );
                assert_eq!(p.preferences.input_offset, Some([2300, 6700]));
                assert!(p.preferences.input_locked);
            });
        }
    }
}

#[test]
fn display_change_during_capture_preserves_resume_but_explicit_stop_cancels_it() {
    let resident = Resident::new();
    let enabled = Enabled {
        halo: true,
        keys: true,
        shapes: false,
        mouse: true,
    };
    for explicit_stop in [false, true] {
        resident.presentation(|p| {
            p.set_enabled(enabled).unwrap();
            p.suspend_capture().unwrap();
        });
        resident.display_change();
        resident.display_change();
        resident.presentation(|p| {
            assert!(!p.enabled.any(), "overlays reappeared over the capture");
            if explicit_stop {
                p.stop().unwrap();
            }
            p.resume_capture().unwrap();
            assert!(
                p.enabled
                    == if explicit_stop {
                        Enabled::default()
                    } else {
                        enabled
                    },
                "display change erased capture restoration"
            );
        });
    }
}

#[test]
fn display_change_inside_menu_invalidates_preparation_after_borrow_ends() {
    let resident = Resident::new();
    resident.state.borrow_mut().prepared =
        Some(crate::session::Preparation::new(80, 60, false).unwrap());
    let enabled = Enabled {
        halo: true,
        ..Default::default()
    };
    resident.presentation(|p| p.set_enabled(enabled).unwrap());
    {
        let borrowed = resident.state.borrow_mut();
        resident.display_change();
        resident.display_change();
        assert!(borrowed.prepared.is_some());
    }
    resident.send(WM_TIMER, editor_events::RETRY);
    assert!(
        resident.state.borrow().prepared.is_none(),
        "menu swallowed display notification"
    );
    resident.presentation(|p| assert!(p.enabled == enabled));
    assert!(editor_events::take(resident.window.0).is_empty());
}
