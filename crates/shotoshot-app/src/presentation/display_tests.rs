//! Rebuild stale native surfaces and relocate an input panel whose old monitor disappeared.
use super::*;

#[test]
fn display_change_keeps_shape_shortcuts_suspended_during_setup() {
    let _com = crate::ComApartment::new().unwrap();
    let owner = crate::resources::OwnedWindow(unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("STATIC"),
            w!("Display setup test"),
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
    let state = Presentation::new(owner.0, PresentationSettings::default()).unwrap();
    let mut p = state.borrow_mut();
    let modifiers = MOD_ALT | MOD_CONTROL | MOD_SHIFT;
    let key = (VK_F13.0..=VK_F24.0)
        .find(|&key| unsafe {
            if RegisterHotKey(Some(owner.0), 200, modifiers, key as u32).is_ok() {
                UnregisterHotKey(Some(owner.0), 200).unwrap();
                true
            } else {
                false
            }
        })
        .expect("a free shortcut for the isolated test");
    p.bindings.shortcuts[0] = crate::settings::Shortcut {
        modifiers: modifiers.0,
        key: key as u32,
    };
    p.set_enabled(Enabled {
        shapes: true,
        ..Default::default()
    })
    .unwrap();
    p.suspend_shortcuts().unwrap();
    p.display_changed().unwrap();
    let free = unsafe { RegisterHotKey(Some(owner.0), 200, modifiers, key as u32) };
    if free.is_ok() {
        unsafe {
            UnregisterHotKey(Some(owner.0), 200).unwrap();
        }
    }
    assert!(
        free.is_ok(),
        "display reconstruction reactivated a suspended shape shortcut"
    );
    assert!(p.enabled.shapes);
    p.stop().unwrap();
}

#[test]
fn display_change_rebuilds_layers_and_returns_keyboard_to_a_work_area() {
    let _com = crate::ComApartment::new().unwrap();
    let owner = crate::resources::OwnedWindow(unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("STATIC"),
            w!("Display reconstruction test"),
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
    let state = Presentation::new(owner.0, PresentationSettings::default()).unwrap();
    let mut p = state.borrow_mut();
    let enabled = Enabled {
        halo: true,
        keys: true,
        shapes: true,
        mouse: false,
    };
    p.set_enabled(enabled).unwrap();
    p.preferences.input_offset = Some([10000, 10000]);
    // Model the old monitor having disappeared, with surfaces sized for the old topology.
    p.input_monitor = POINT {
        x: -1_000_000,
        y: -1_000_000,
    };
    p.halo = Some(Layer::new(p.address, -32000, -32000, 10, 10).unwrap());
    p.keys = Some(Layer::new(p.address, -32000, -32000, 10, 10).unwrap());
    p.canvas = Some(Layer::new(p.address, -32000, -32000, 10, 10).unwrap());
    p.scene.arm(LiveTool::Rectangle);
    p.scene.begin(Point { x: 10., y: 10. });
    p.scene.update(Point { x: 80., y: 80. }, false, false);
    assert!(p.scene.intercepts_input());

    p.display_changed().unwrap();

    assert!(p.enabled == enabled);
    assert!(p.scene.draft.is_none() && !p.scene.intercepts_input());
    assert!(p.canvas.is_none(), "stale desktop canvas survived");
    let keys = p.keys.as_ref().unwrap();
    assert!(keys.width > 10 && p.halo.as_ref().unwrap().width > 10);
    assert!(!keys.intercepts(), "locked keyboard became interactive");
    assert_eq!(p.preferences.input_offset, Some([10000, 10000]));
    let mut bounds = RECT::default();
    let mut monitor = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    unsafe {
        GetWindowRect(keys.hwnd(), &mut bounds).unwrap();
        GetMonitorInfoW(
            MonitorFromPoint(p.input_monitor, MONITOR_DEFAULTTONEAREST),
            &mut monitor,
        )
        .ok()
        .unwrap();
        assert!(IsWindowVisible(keys.hwnd()).as_bool());
    }
    assert_eq!(bounds.right, monitor.rcWork.right);
    assert_eq!(bounds.bottom, monitor.rcWork.bottom);
    assert!(bounds.left >= monitor.rcWork.left && bounds.top >= monitor.rcWork.top);
    p.stop().unwrap();
}
