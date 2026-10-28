use super::*;

#[test]
fn live_shapes_have_soft_corners_and_independently_sized_arrowheads() {
    use shotoshot_core::{Object, Rect, Shape, Style};
    let _com = crate::ComApartment::new().unwrap();
    let layer = Layer::new(std::ptr::null(), -32000, -32000, 220, 140).unwrap();
    layer.begin(false).unwrap();
    layer.object(
        &Object {
            rotation: 0.,
            fill: Vec::new(),
            id: 0,
            style: Style {
                width: 8.,
                ..Default::default()
            },
            shape: Shape::Rectangle(Rect {
                min: Point { x: 20., y: 20. },
                max: Point { x: 190., y: 120. },
            }),
        },
        1.,
        28.,
    );
    layer.end().unwrap();
    // A sharp corner would be opaque here. The GPU leaves 0; Direct2D's software rasterizer,
    // used on machines without a GPU (GitHub Actions), spills 4 of 255 of antialiasing.
    assert!(layer.pixel(20, 20)[3] < 16, "the corner should be softened");
    assert!(
        layer.pixel(20, 60)[3] > 200,
        "straight sides keep their full thickness"
    );
    let arrow = Object {
        rotation: 0.,
        fill: Vec::new(),
        id: 0,
        style: Style {
            width: 5.,
            ..Default::default()
        },
        shape: Shape::Arrow {
            start: Point { x: 20., y: 60. },
            end: Point { x: 180., y: 60. },
            start_attach: None,
            end_attach: None,
            head: true,
        },
    };
    layer.begin(false).unwrap();
    layer.object(&arrow, 1., 16.);
    layer.end().unwrap();
    assert_eq!(layer.pixel(158, 55)[3], 0);
    let shaft = layer.pixel(80, 60);
    layer.begin(false).unwrap();
    layer.object(&arrow, 1., 40.);
    layer.end().unwrap();
    assert!(
        layer.pixel(158, 55)[3] > 200,
        "a larger head must contain an opaque triangle interior"
    );
    assert!(
        layer.pixel(155, 68)[3] > 200,
        "the head should cover its shoulder"
    );
    assert_eq!(
        layer.pixel(150, 71)[3],
        0,
        "the reduced head must leave its former shoulder empty"
    );
    assert_eq!(
        shaft,
        layer.pixel(80, 60),
        "head size must not change shaft width"
    );
}

#[test]
fn fading_arrow_has_no_double_opacity_at_the_head_join() {
    use shotoshot_core::{Object, Shape, Style};
    let _com = crate::ComApartment::new().unwrap();
    let layer = Layer::new(std::ptr::null(), -32000, -32000, 240, 160).unwrap();
    for end in [Point { x: 220., y: 80. }, Point { x: 180., y: 130. }] {
        let arrow = Object {
            rotation: 0.,
            fill: Vec::new(),
            id: 0,
            style: Style {
                width: 10.,
                ..Default::default()
            },
            shape: Shape::Arrow {
                start: Point { x: 20., y: 80. },
                end,
                start_attach: None,
                end_attach: None,
                head: true,
            },
        };
        layer.begin(false).unwrap();
        layer.object(&arrow, 0.5, 56.);
        layer.end().unwrap();
        for y in 0..160 {
            for x in 0..240 {
                assert!(layer.pixel(x, y)[3] <= 128, "double opacity at {x},{y}");
            }
        }
        assert!(layer.pixel(18, 80)[3] > 100, "round tail lost");
        let (x, y) = if end.y == 80. { (181, 80) } else { (144, 119) };
        assert!(
            (127..=128).contains(&layer.pixel(x, y)[3]),
            "lighter seam at the head join"
        );
    }
}

#[test]
fn physical_keys_render_press_release_and_pause_without_an_idle_timer() {
    let _com = crate::ComApartment::new().unwrap();
    let owner = crate::resources::OwnedWindow(unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("STATIC"),
            w!("Input diagram test"),
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
    assert!(!p.enabled.keys);
    // Key events only reach a keyboard that is shown.
    p.enabled.keys = true;
    p.keys = Some(Layer::new(&*state, -32000, -32000, diagram::WIDTH, diagram::HEIGHT).unwrap());
    let hwnd = p.keys.as_ref().unwrap().hwnd();
    p.paint(hwnd).unwrap();
    let idle = p.keys.as_ref().unwrap().pixel(10, 119);
    p.keyboard(RAWKEYBOARD {
        MakeCode: 29,
        VKey: VK_CONTROL.0,
        ..Default::default()
    });
    p.paint(hwnd).unwrap();
    let pressed = p.keys.as_ref().unwrap().pixel(10, 119);
    assert_ne!(idle, pressed);
    p.sync_timer().unwrap();
    assert!(!p.timer, "a held key does not need animation");
    p.keyboard(RAWKEYBOARD {
        MakeCode: 29,
        VKey: VK_CONTROL.0,
        Flags: 1,
        ..Default::default()
    });
    p.sync_timer().unwrap();
    assert!(p.timer, "modifier labels fade after release");
    p.paint(hwnd).unwrap();
    // The selection colour leaves at once; the ghost key then fades out.
    let released = p.keys.as_ref().unwrap().pixel(10, 119);
    assert!(released != pressed && released != idle);
    let now = p.now();
    p.devices.keyboard(46, false, true, now);
    p.devices.set_label(46, false, &[67]);
    p.devices.keyboard(46, false, false, now);
    p.sync_timer().unwrap();
    assert!(p.timer, "released letters animate their fade");
    p.keyboard(RAWKEYBOARD {
        MakeCode: 69,
        VKey: VK_PAUSE.0,
        Flags: 4,
        ..Default::default()
    });
    assert!(!p.devices.active(69, true, p.now()));
    // Two seconds later, however long a loaded machine took to get here.
    p.clock -= std::time::Duration::from_secs(2);
    p.tick().unwrap();
    p.paint(hwnd).unwrap();
    assert!(!p.timer);
    assert!(
        !p.devices.active(69, true, p.now()),
        "Pause has no break event and must not stay lit"
    );
    assert_eq!(idle, p.keys.as_ref().unwrap().pixel(10, 119));
}

#[test]
fn native_layer_releases_input_without_removing_visible_annotation() {
    let _com = crate::ComApartment::new().unwrap();
    let layer = layer::Layer::new(std::ptr::null(), -32000, -32000, 180, 120).unwrap();
    layer.intercept(true).unwrap();
    assert!(layer.intercepts());
    layer.begin(true).unwrap();
    layer.object(
        &shotoshot_core::Object {
            rotation: 0.,
            fill: Vec::new(),
            id: 1,
            shape: shotoshot_core::Shape::Rectangle(shotoshot_core::Rect {
                min: Point { x: 20., y: 20. },
                max: Point { x: 150., y: 90. },
            }),
            style: shotoshot_core::Style::default(),
        },
        1.,
        28.,
    );
    layer.end().unwrap();
    layer.intercept(false).unwrap();
    assert!(
        !layer.intercepts(),
        "pointer mode must pass input to other apps"
    );
    assert!(
        layer.pixel(20, 50)[3] > 100,
        "returning input erased the drawing"
    );
}

#[test]
fn native_gesture_release_cancel_expire_and_stop_return_to_idle() {
    let _com = crate::ComApartment::new().unwrap();
    unsafe {
        let owner = crate::resources::OwnedWindow(
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                w!("STATIC"),
                w!("Presentation test"),
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
        let state = Presentation::new(owner.0, PresentationSettings::default()).unwrap();
        let canvas = Layer::new(&*state, -32000, -32000, 300, 200).unwrap();
        let hwnd = canvas.hwnd();
        canvas.intercept(true).unwrap();
        {
            let mut p = state.borrow_mut();
            p.canvas = Some(canvas);
            p.enabled.shapes = true;
            p.scene.arm(LiveTool::Rectangle);
        }
        let send = |msg, x: u16, y: u16| {
            SendMessageW(
                hwnd,
                msg,
                Some(WPARAM(0)),
                Some(LPARAM(((u32::from(y) << 16) | u32::from(x)) as isize)),
            );
        };
        send(WM_LBUTTONDOWN, 20, 20);
        send(WM_MOUSEMOVE, 160, 100);
        send(WM_LBUTTONUP, 160, 100);
        {
            let mut p = state.borrow_mut();
            assert_eq!(p.scene.shapes.len(), 1);
            assert!(!p.scene.intercepts_input());
            assert!(!p.canvas.as_ref().unwrap().intercepts());
            assert_ne!(GetCapture(), hwnd);
            assert!(p.timer, "the shape must expire even without further input");
            p.scene.arm(LiveTool::Arrow);
            p.canvas.as_ref().unwrap().intercept(true).unwrap();
        }
        send(WM_LBUTTONDOWN, 30, 30);
        send(WM_MOUSEMOVE, 80, 80);
        let (shape, width, head) = {
            let p = state.borrow();
            let draft = p.scene.draft.as_ref().unwrap();
            (draft.shape.clone(), draft.style.width, p.scene.arrow_size)
        };
        for delta in [120_i16, 120, -120, -120] {
            SendMessageW(
                hwnd,
                WM_MOUSEWHEEL,
                Some(WPARAM((u32::from(delta as u16) << 16) as usize)),
                Some(LPARAM(0)),
            );
            let p = state.borrow();
            let draft = p.scene.draft.as_ref().unwrap();
            assert_eq!(draft.shape, shape, "wheel moved the arrow endpoints");
            assert!(
                (p.scene.arrow_size / head - draft.style.width / width).abs() < 1e-8,
                "wheel scaled the shaft without the head"
            );
        }
        send(WM_CANCELMODE, 0, 0);
        {
            let mut p = state.borrow_mut();
            assert_eq!(
                p.scene.shapes.len(),
                1,
                "cancelling must keep the earlier annotation"
            );
            assert!(!p.canvas.as_ref().unwrap().intercepts());
            // Four seconds later, however long a loaded machine took to draw the shape.
            p.clock -= std::time::Duration::from_secs(4);
            p.tick().unwrap();
            assert!(p.scene.shapes.is_empty());
            assert!(
                p.canvas.is_none(),
                "release the desktop-size surface after expiration"
            );
            assert!(!p.timer);
            let empty = Layer::new(&*state, -32000, -32000, 300, 200).unwrap();
            p.canvas = Some(empty);
            p.scene.arm(LiveTool::Arrow);
            p.release_gesture().unwrap();
            assert!(
                p.canvas.is_none(),
                "cancelling without a shape must release the large bitmap without waiting for a timer"
            );
            p.stop().unwrap();
            assert!(!p.enabled.any());
            assert!(!p.timer);
            assert!(p.halo.is_none() && p.keys.is_none());
        }
    }
}

#[test]
fn alt_f4_cancels_the_gesture_without_leaving_a_dead_canvas() {
    let _com = crate::ComApartment::new().unwrap();
    let owner = crate::resources::OwnedWindow(
        unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                w!("STATIC"),
                w!("Close test"),
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
        }
        .unwrap(),
    );
    let state = Presentation::new(owner.0, PresentationSettings::default()).unwrap();
    let canvas = Layer::new(&*state, -32000, -32000, 300, 200).unwrap();
    let hwnd = canvas.hwnd();
    {
        let mut p = state.borrow_mut();
        p.canvas = Some(canvas);
        p.scene.arm(LiveTool::Rectangle);
    }
    unsafe {
        SendMessageW(hwnd, WM_CLOSE, None, None);
    }
    let p = state.borrow();
    assert!(!p.scene.intercepts_input());
    assert!(
        p.canvas.is_none() || unsafe { IsWindow(Some(hwnd)).as_bool() },
        "WM_CLOSE left a destroyed HWND in the presentation controller"
    );
}

#[test]
fn halo_click_remains_visible_when_the_chosen_color_is_already_coral() {
    let _com = crate::ComApartment::new().unwrap();
    let layer = Layer::new(std::ptr::null(), -32000, -32000, 72, 72).unwrap();
    layer.begin(false).unwrap();
    layer.halo([237, 79, 92], false, None);
    layer.end().unwrap();
    let before = layer.pixel(36, 36);
    layer.begin(false).unwrap();
    layer.halo([237, 79, 92], true, None);
    layer.end().unwrap();
    assert_ne!(
        before,
        layer.pixel(36, 36),
        "click feedback disappeared for the coral palette option"
    );
}

#[test]
fn capture_restores_keyboard_and_lock_but_explicit_stop_cancels_restore() {
    let _com = crate::ComApartment::new().unwrap();
    let owner = crate::resources::OwnedWindow(unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("STATIC"),
            w!("Capture restore test"),
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
    p.enabled.keys = true;
    p.preferences.input_offset = Some([1000, 2000]);
    p.input_monitor = POINT { x: 100, y: 100 };
    p.suspend_capture().unwrap();
    p.suspend_capture().unwrap();
    assert!(!p.enabled.any());
    assert!(p.keys.is_none() && p.lock.is_none());
    p.resume_capture().unwrap();
    assert!(p.enabled.keys);
    assert!(
        p.keys.is_some() && p.lock.is_none(),
        "a resumed diagram is a still ghost"
    );
    assert_eq!(p.preferences.input_offset, Some([1000, 2000]));
    assert_eq!((p.input_monitor.x, p.input_monitor.y), (100, 100));
    p.suspend_capture().unwrap();
    p.stop().unwrap();
    p.resume_capture().unwrap();
    assert!(!p.enabled.any());
}

#[test]
fn iso_enter_has_one_outline_and_clear_gaps_to_neighbours() {
    let _com = crate::ComApartment::new().unwrap();
    let layer = Layer::new(
        std::ptr::null(),
        -32000,
        -32000,
        diagram::WIDTH,
        diagram::HEIGHT,
    )
    .unwrap();
    let mut devices = Devices::default();
    devices.keyboard(28, false, true, 0);
    layer.begin(false).unwrap();
    layer.devices(&devices, 0, true);
    layer.end().unwrap();
    // Both arms share one uninterrupted fill, without a rectangle border at y=65.
    assert_eq!(layer.pixel(271, 62), layer.pixel(271, 65));
    assert_eq!(layer.pixel(271, 65), layer.pixel(271, 69));
    assert!(layer.pixel(260, 51)[3] < 100, "gap below Backspace");
    assert!(layer.pixel(254, 77)[3] < 100, "gap to the adjacent ISO key");
    let iso_upper_enter = layer.pixel(271, 62);
    devices.ansi = true;
    layer.begin(false).unwrap();
    layer.devices(&devices, 0, true);
    layer.end().unwrap();
    assert_ne!(
        layer.pixel(271, 62),
        iso_upper_enter,
        "ANSI Enter has no upper arm"
    );
    assert_eq!(
        layer.pixel(245, 77),
        layer.pixel(271, 77),
        "ANSI Enter spans the home row"
    );
}

#[test]
fn observed_combinations_keep_all_keys_and_release_independently() {
    let _com = crate::ComApartment::new().unwrap();
    let owner = crate::resources::OwnedWindow(unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("STATIC"),
            w!("Combo test"),
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
    state.borrow_mut().enabled.keys = true;
    let key = |vk: u16, scan: u16, up: bool| {
        events::resident_message(
            owner.0,
            OBSERVED_KEY,
            WPARAM(vk as usize),
            LPARAM((u32::from(scan) | (u32::from(up) << 16)) as isize),
        );
    };
    key(0x11, 29, false);
    key(0x10, 42, false);
    key(0x20, 57, false);
    {
        let p = state.borrow();
        for scan in [29, 42, 57] {
            assert!(p.devices.active(scan, false, p.now()));
            assert!(!p.devices.label(scan, false, p.now()).0.is_empty());
        }
    }
    key(0x20, 57, true);
    {
        let p = state.borrow();
        assert!(!p.devices.active(57, false, p.now()));
        assert!(p.devices.active(29, false, p.now()));
        assert!(!p.devices.label(57, false, p.now()).0.is_empty());
    }
    key(0x10, 42, true);
    key(0x11, 29, true);
    key(0x12, 56, false);
    key(0x46, 33, false);
    {
        let p = state.borrow();
        assert!(p.devices.active(56, false, p.now()));
        assert!(p.devices.active(33, false, p.now()));
        assert!(!p.devices.label(33, false, p.now()).0.is_empty());
    }
    key(0x46, 33, true);
    key(0x12, 56, true);
    state.borrow_mut().stop().unwrap();
}

#[test]
fn thumb_buttons_follow_bottom_back_and_top_forward_positions() {
    let _com = crate::ComApartment::new().unwrap();
    let layer = Layer::new(
        std::ptr::null(),
        -32000,
        -32000,
        diagram::WIDTH,
        diagram::HEIGHT,
    )
    .unwrap();
    let mut devices = Devices::default();
    devices.mouse(64, 0); // XBUTTON1 / Back, bottom button.
    layer.begin(false).unwrap();
    layer.devices(&devices, 0, true);
    layer.end().unwrap();
    assert!(layer.pixel(451, 84)[0] > layer.pixel(451, 64)[0]);
    devices.clear();
    devices.mouse(256, 0); // XBUTTON2 / Forward, top button.
    layer.begin(false).unwrap();
    layer.devices(&devices, 0, true);
    layer.end().unwrap();
    assert!(layer.pixel(451, 64)[0] > layer.pixel(451, 84)[0]);
}
#[test]
fn key_release_during_reentrant_foreground_change_is_not_lost() {
    let _com = crate::ComApartment::new().unwrap();
    let owner = crate::resources::OwnedWindow(unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("STATIC"),
            w!("Deferred input"),
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
    {
        let mut p = state.borrow_mut();
        p.enabled.keys = true;
        p.devices.keyboard(56, false, true, 0);
        p.devices.keyboard(33, false, true, 0);
        for (vk, scan) in [(0x12, 56), (0x46, 33)] {
            events::resident_message(owner.0, OBSERVED_KEY, WPARAM(vk), LPARAM(scan | (1 << 16)));
        }
        assert!(p.devices.active(56, false, 0));
        assert!(p.devices.active(33, false, 0));
    }
    unsafe {
        let mut message = MSG::default();
        while PeekMessageW(
            &mut message,
            Some(owner.0),
            OBSERVED_KEY,
            OBSERVED_KEY,
            PM_REMOVE,
        )
        .as_bool()
        {
            events::resident_message(owner.0, message.message, message.wParam, message.lParam);
        }
    }
    let mut p = state.borrow_mut();
    assert!(!p.devices.active(56, false, 0));
    assert!(!p.devices.active(33, false, 0));
    p.stop().unwrap();
}

fn plain_owner(title: PCWSTR) -> crate::resources::OwnedWindow {
    crate::resources::OwnedWindow(unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("STATIC"),
            title,
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
    })
}

#[test]
fn a_key_outside_the_keyboard_is_tracked_without_lighting_or_animating() {
    let _com = crate::ComApartment::new().unwrap();
    let owner = plain_owner(w!("Hidden key test"));
    let state = Presentation::new(owner.0, PresentationSettings::default()).unwrap();
    let mut p = state.borrow_mut();
    p.enabled.keys = true;
    let key = |vk: u16, scan: u16, up: bool| RAWKEYBOARD {
        VKey: vk,
        MakeCode: scan,
        Flags: u16::from(up),
        ..Default::default()
    };
    // F13 has no key on the drawn keyboard.
    assert!(!p.keyboard(key(0x7c, 0x64, false)));
    assert!(
        p.devices.active(0x64, false, 0),
        "a held hidden key is still known"
    );
    assert!(p.devices.label(0x64, false, p.now()).0.is_empty());
    assert!(!p.keyboard(key(0x7c, 0x64, true)));
    assert!(
        !p.devices.needs_animation(p.now()),
        "a hidden key must not start the fade timer"
    );
    assert!(p.keyboard(key(0x4a, 36, false)), "J is drawn");
    assert!(p.devices.active(36, false, 0));
    assert!(p.keyboard(key(0x4a, 36, true)));
}

#[test]
fn the_diagram_window_follows_the_shown_devices_and_the_size() {
    let _com = crate::ComApartment::new().unwrap();
    let owner = plain_owner(w!("Diagram plan test"));
    let state = Presentation::new(owner.0, PresentationSettings::default()).unwrap();
    let mut p = state.borrow_mut();
    let both = Enabled {
        keys: true,
        mouse: true,
        ..Default::default()
    };
    p.set_enabled(both).unwrap();
    let monitor = monitor_scale(p.input_monitor);
    let width_of = |plan: diagram::Plan, size: f64| (plan.width() as f64 * monitor * size) as i32;
    let shown = |p: &Presentation| {
        let keys = p.keys.as_ref().unwrap();
        (keys.plan(), keys.width)
    };
    let full = diagram::Plan::FULL;
    assert_eq!(shown(&p), (full, width_of(full, 1.)));

    let keyboard = diagram::Plan {
        mouse: false,
        ..full
    };
    p.set_enabled(Enabled {
        mouse: false,
        ..both
    })
    .unwrap();
    assert_eq!(shown(&p), (keyboard, width_of(keyboard, 1.)));

    let mouse = diagram::Plan {
        keyboard: false,
        ..full
    };
    p.set_enabled(Enabled {
        keys: false,
        ..both
    })
    .unwrap();
    assert_eq!(shown(&p), (mouse, width_of(mouse, 1.)));
    assert!(
        p.lock.is_none(),
        "a still diagram has no lock to catch clicks"
    );

    p.set_enabled(both).unwrap();
    let preferences = PresentationSettings {
        input_scale: 150,
        ..p.preferences.clone()
    };
    p.update_preferences(preferences).unwrap();
    assert_eq!(shown(&p), (full, width_of(full, 1.5)));

    // During a move, the lock stays on its key place of the resized diagram.
    p.start_move().unwrap();
    let (mut diagram_at, mut lock_at) = (RECT::default(), RECT::default());
    unsafe {
        GetWindowRect(p.keys.as_ref().unwrap().hwnd(), &mut diagram_at).unwrap();
        GetWindowRect(p.lock.as_ref().unwrap().hwnd(), &mut lock_at).unwrap();
    }
    let scale = shown(&p).1 as f64 / full.width() as f64;
    assert_eq!(
        (lock_at.left - diagram_at.left, lock_at.top - diagram_at.top),
        (
            (full.lock().0 as f64 * scale) as i32,
            (full.lock().1 as f64 * scale) as i32
        )
    );
    assert_eq!(
        lock_at.right - lock_at.left,
        (diagram::LOCK_SIZE as f64 * scale) as i32
    );
    p.stop().unwrap();
    assert!(p.keys.is_none() && p.lock.is_none());
}

#[test]
fn a_lone_mouse_shows_its_click_without_a_keyboard_panel() {
    let _com = crate::ComApartment::new().unwrap();
    let mut devices = Devices::default();
    devices.mouse(1, 0);
    let alone = diagram::Plan {
        keyboard: false,
        ..diagram::Plan::FULL
    };
    let layer = Layer::diagram(std::ptr::null(), alone, 1.).unwrap();
    layer.begin(false).unwrap();
    layer.devices(&devices, 0, true);
    layer.end().unwrap();
    assert_eq!(layer.width, alone.width());
    assert!(layer.pixel(22, 50)[0] > 200, "a lone mouse shows its click");
    assert_eq!(layer.pixel(3, 130)[3], 0, "and no keyboard panel");
}

#[test]
fn the_pointer_layer_draws_the_halo_and_its_trail_where_they_are() {
    use shotoshot_core::pointer::{Effects, HaloShape, PointerSettings, TrailStyle};
    let _com = crate::ComApartment::new().unwrap();
    let layer = Layer::new(std::ptr::null(), -32000, -32000, 200, 200).unwrap();
    let settings = PointerSettings {
        halo_shape: HaloShape::Ring,
        halo_thickness: 4,
        trail: TrailStyle::Line,
        ..Default::default()
    };
    // The layer sits at (1000, 500) on screen; the pointer went from (1020, 600) to (1100, 600).
    let origin = Point { x: 1000., y: 500. };
    let mut effects = Effects::default();
    for step in 0..=8u64 {
        let at = Point {
            x: 1020. + step as f64 * 10.,
            y: 600.,
        };
        effects.moved(at, step * 10, 1., &settings);
    }
    let halo = shotoshot_ui::pointer::HaloState {
        color: [255, 211, 64],
        pressed: false,
        pulse: None,
    };
    layer.begin(false).unwrap();
    layer
        .pointer(origin, &effects, &settings, halo, 1., 80)
        .unwrap();
    layer.end().unwrap();
    let alpha = |x, y| layer.pixel(x, y)[3];
    assert!(
        alpha(100 + 24, 100) > 150,
        "the ring, 24 px right of the pointer"
    );
    assert_eq!(alpha(100, 88), 0, "a ring is empty inside");
    assert!(alpha(50, 100) > 50, "the trail behind the pointer");
    assert_eq!(alpha(100, 40), 0, "nothing above");
}

#[test]
fn a_region_frame_redraws_and_sends_only_its_area() {
    let _com = crate::ComApartment::new().unwrap();
    let layer = Layer::new(std::ptr::null(), -32000, -32000, 300, 200).unwrap();
    let paint = |dirty: RECT, x: f64| {
        layer.begin_region(dirty).unwrap();
        let mut effects = shotoshot_core::pointer::Effects::default();
        effects.place(Point { x, y: 100. }, 0);
        layer
            .pointer(
                Point { x: 0., y: 0. },
                &effects,
                &shotoshot_core::pointer::PointerSettings::default(),
                shotoshot_ui::pointer::HaloState {
                    color: [255, 211, 64],
                    pressed: false,
                    pulse: None,
                },
                1.,
                0,
            )
            .unwrap();
        layer.end_region(dirty).unwrap();
    };
    let left = RECT {
        left: 0,
        top: 0,
        right: 150,
        bottom: 200,
    };
    paint(left, 75.);
    assert!(layer.pixel(75 + 24, 100)[3] > 0, "the halo's edge, drawn");
    // A frame over the right half only: the left halo stays, the new one appears.
    let right = RECT {
        left: 150,
        top: 0,
        right: 300,
        bottom: 200,
    };
    paint(right, 225.);
    assert!(
        layer.pixel(75 + 24, 100)[3] > 0,
        "outside the region, untouched"
    );
    assert!(layer.pixel(225 + 24, 100)[3] > 0, "inside, redrawn");
    // Clearing the left half alone erases the old halo there.
    layer.begin_region(left).unwrap();
    layer.end_region(left).unwrap();
    assert_eq!(layer.pixel(75 + 24, 100)[3], 0, "erased");
    assert!(layer.pixel(225 + 24, 100)[3] > 0, "the other half kept");
}

/// The box around every pixel at least `alpha` opaque, None on a blank layer.
fn opaque_bounds(layer: &Layer, alpha: u8) -> Option<[u32; 4]> {
    let mut bounds: Option<[u32; 4]> = None;
    for y in 0..layer.height as u32 {
        for x in 0..layer.width as u32 {
            if layer.pixel(x, y)[3] >= alpha {
                let b = bounds.get_or_insert([x, y, x, y]);
                *b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
            }
        }
    }
    bounds
}

#[test]
fn a_still_diagram_is_a_ghost_showing_only_the_pressed_key_then_nothing() {
    let _com = crate::ComApartment::new().unwrap();
    let layer = Layer::diagram(std::ptr::null(), diagram::Plan::FULL, 1.).unwrap();
    let paint = |devices: &Devices, now, whole| {
        layer.begin(false).unwrap();
        layer.devices(devices, now, whole);
        layer.end().unwrap();
    };
    let mut devices = Devices::default();
    paint(&devices, 0, false);
    assert_eq!(opaque_bounds(&layer, 1), None, "nothing at rest");
    paint(&devices, 0, true);
    assert!(
        opaque_bounds(&layer, 1).is_some(),
        "the whole diagram while moved"
    );

    devices.keyboard(30, false, true, 0); // A
    devices.set_label(30, false, &[65]);
    paint(&devices, 0, false);
    let [left, top, right, bottom] = opaque_bounds(&layer, 120).unwrap();
    assert!(
        right - left < diagram::HEIGHT as u32 / 6 && bottom - top < diagram::HEIGHT as u32 / 6,
        "only one keycap is solid: {left},{top} to {right},{bottom}"
    );
    let outline = opaque_bounds(&layer, 1).unwrap();
    assert!(
        outline[2] - outline[0] > layer.width as u32 / 2,
        "a pale outline frames the keyboard while a key is held"
    );
    assert!(
        (0..layer.height as u32).all(|y| layer.pixel(1, y)[3] < 80),
        "the outline stays pale"
    );

    devices.keyboard(30, false, false, 100);
    paint(&devices, 400, false);
    let fading = opaque_bounds(&layer, 1).unwrap();
    assert!(
        fading[2] - fading[0] > layer.width as u32 / 2,
        "key and outline fade together"
    );
    assert!(devices.needs_animation(400));
    paint(&devices, 700, false);
    assert_eq!(opaque_bounds(&layer, 1), None, "gone after the fade");
    assert!(!devices.needs_animation(700), "no timer once gone");

    // A key without a label still shows, and still fades out.
    devices.keyboard(58, true, true, 1000);
    devices.keyboard(58, true, false, 1000);
    assert!(devices.needs_animation(1300));
}

#[test]
fn a_ghost_mouse_shows_only_the_pressed_button() {
    let _com = crate::ComApartment::new().unwrap();
    let plan = diagram::Plan {
        keyboard: false,
        ..diagram::Plan::FULL
    };
    let layer = Layer::diagram(std::ptr::null(), plan, 1.).unwrap();
    let solid = |devices: &Devices| {
        layer.begin(false).unwrap();
        layer.devices(devices, 0, false);
        layer.end().unwrap();
        opaque_bounds(&layer, 120)
    };
    let mut devices = Devices::default();
    assert_eq!(solid(&devices), None, "nothing at rest");
    devices.mouse(1, 0); // left down
    let left = solid(&devices).unwrap();
    devices.mouse(2 | 4, 0); // left up, right down
    devices.clear();
    devices.mouse(4, 0);
    let right = solid(&devices).unwrap();
    assert!(
        left[2] < right[0],
        "left {left:?} and right {right:?} buttons stand apart"
    );
}

#[test]
fn moving_shows_the_whole_diagram_until_the_lock_enter_or_esc() {
    let _com = crate::ComApartment::new().unwrap();
    let owner = plain_owner(w!("Diagram move test"));
    let state = Presentation::new(owner.0, PresentationSettings::default()).unwrap();
    let mut p = state.borrow_mut();
    // Asking for a move with nothing shown brings the keyboard.
    p.command(106).unwrap();
    assert!(p.enabled.keys && p.moving);
    assert!(
        p.keys.as_ref().unwrap().intercepts(),
        "the moved diagram is draggable"
    );
    assert!(p.lock.is_some(), "its lock ends the move");

    let esc = |up| RAWKEYBOARD {
        VKey: VK_ESCAPE.0,
        MakeCode: 1,
        Flags: u16::from(up),
        ..Default::default()
    };
    p.key_event(esc(false)).unwrap();
    assert!(!p.moving, "Esc ends the move");
    assert!(
        !p.keys.as_ref().unwrap().intercepts(),
        "a still diagram lets every click through"
    );
    assert!(p.lock.is_none());
    p.key_event(esc(true)).unwrap();
    p.key_event(esc(false)).unwrap();
    assert!(
        !p.moving && p.lock.is_none(),
        "Esc outside a move changes nothing"
    );

    p.command(106).unwrap();
    let lock = p.lock.as_ref().unwrap().hwnd();
    p.input_window_message(lock, WM_LBUTTONDOWN).unwrap();
    assert!(!p.moving && p.lock.is_none(), "the lock ends the move");

    p.command(106).unwrap();
    p.stop().unwrap();
    assert!(!p.moving, "turning everything off ends the move");
}
