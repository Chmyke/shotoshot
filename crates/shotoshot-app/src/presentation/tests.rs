use super::*;
use crate::settings::InputLayout;

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
    assert_ne!(idle, p.keys.as_ref().unwrap().pixel(10, 119));
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
    assert_eq!(idle, p.keys.as_ref().unwrap().pixel(10, 119));
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
    p.preferences.input_locked = true;
    p.preferences.input_offset = Some([1000, 2000]);
    p.input_monitor = POINT { x: 100, y: 100 };
    p.suspend_capture().unwrap();
    p.suspend_capture().unwrap();
    assert!(!p.enabled.any());
    assert!(p.keys.is_none() && p.lock.is_none());
    p.resume_capture().unwrap();
    assert!(p.enabled.keys);
    assert!(p.keys.is_some() && p.lock.is_some());
    assert!(p.preferences.input_locked);
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
    layer.devices(&devices, 0);
    layer.end().unwrap();
    // Both arms share one uninterrupted fill, without a rectangle border at y=65.
    assert_eq!(layer.pixel(271, 62), layer.pixel(271, 65));
    assert_eq!(layer.pixel(271, 65), layer.pixel(271, 69));
    assert!(layer.pixel(260, 51)[3] < 100, "gap below Backspace");
    assert!(layer.pixel(254, 77)[3] < 100, "gap to the adjacent ISO key");
    let iso_upper_enter = layer.pixel(271, 62);
    devices.ansi = true;
    layer.begin(false).unwrap();
    layer.devices(&devices, 0);
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
    layer.devices(&devices, 0);
    layer.end().unwrap();
    assert!(layer.pixel(451, 84)[0] > layer.pixel(451, 64)[0]);
    devices.clear();
    devices.mouse(256, 0); // XBUTTON2 / Forward, top button.
    layer.begin(false).unwrap();
    layer.devices(&devices, 0);
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
fn a_key_outside_the_layout_is_tracked_without_lighting_or_animating() {
    let _com = crate::ComApartment::new().unwrap();
    let owner = plain_owner(w!("Hidden key test"));
    let preferences = PresentationSettings {
        input_layout: InputLayout::Gaming,
        ..Default::default()
    };
    let state = Presentation::new(owner.0, preferences).unwrap();
    let mut p = state.borrow_mut();
    p.enabled.keys = true;
    let key = |vk: u16, scan: u16, up: bool| RAWKEYBOARD {
        VKey: vk,
        MakeCode: scan,
        Flags: u16::from(up),
        ..Default::default()
    };
    // Right Shift and J lie outside the left-hand block.
    assert!(!p.keyboard(key(0xa1, 54, false)));
    assert!(!p.keyboard(key(0x4a, 36, false)));
    assert!(
        p.devices.active(54, false, 0),
        "a held hidden modifier still shapes the labels"
    );
    assert!(p.devices.label(36, false, p.now()).0.is_empty());
    assert!(!p.keyboard(key(0x4a, 36, true)));
    assert!(!p.keyboard(key(0xa1, 54, true)));
    assert!(
        !p.devices.needs_animation(p.now()),
        "a hidden key must not start the fade timer"
    );
    assert!(p.keyboard(key(0x57, 17, false)), "W belongs to the block");
    assert!(p.devices.active(17, false, 0));
    assert!(p.keyboard(key(0x57, 17, true)));
    // The full keyboard shows the same key again.
    p.preferences.input_layout = InputLayout::Full;
    assert!(p.keyboard(key(0x4a, 36, false)));
    assert!(p.keyboard(key(0x4a, 36, true)));
}

#[test]
fn the_diagram_window_follows_the_shown_devices_the_layout_and_the_size() {
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
    assert!(p.lock.is_some(), "a lone mouse keeps its lock");

    p.set_enabled(both).unwrap();
    let preferences = PresentationSettings {
        input_layout: InputLayout::Gaming,
        input_scale: 150,
        ..p.preferences.clone()
    };
    p.update_preferences(preferences).unwrap();
    let gaming = diagram::Plan {
        layout: InputLayout::Gaming,
        ..full
    };
    assert_eq!(shown(&p), (gaming, width_of(gaming, 1.5)));
    assert!(width_of(gaming, 1.5) < width_of(full, 1.));

    // The lock stays on its key place of the resized diagram.
    let (mut diagram_at, mut lock_at) = (RECT::default(), RECT::default());
    unsafe {
        GetWindowRect(p.keys.as_ref().unwrap().hwnd(), &mut diagram_at).unwrap();
        GetWindowRect(p.lock.as_ref().unwrap().hwnd(), &mut lock_at).unwrap();
    }
    let scale = shown(&p).1 as f64 / gaming.width() as f64;
    assert_eq!(
        (lock_at.left - diagram_at.left, lock_at.top - diagram_at.top),
        (
            (gaming.lock().0 as f64 * scale) as i32,
            (gaming.lock().1 as f64 * scale) as i32
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
fn gaming_draws_the_left_hand_block_with_the_mouse_right_beside_it() {
    let _com = crate::ComApartment::new().unwrap();
    let plan = diagram::Plan {
        layout: InputLayout::Gaming,
        ..diagram::Plan::FULL
    };
    let layer = Layer::diagram(std::ptr::null(), plan, 1.).unwrap();
    assert_eq!((layer.width, layer.height), (plan.width(), diagram::HEIGHT));
    let mut devices = Devices::default();
    devices.keyboard(17, false, true, 0); // W
    devices.mouse(1, 0);
    layer.begin(false).unwrap();
    layer.devices(&devices, 0);
    layer.end().unwrap();
    assert!(layer.pixel(55, 55)[0] > 200, "W is lit");
    assert!(layer.pixel(73, 55)[0] < 100, "E is not");
    assert_eq!(layer.pixel(150, 60)[3], 0, "nothing after the block");
    assert!(layer.pixel(178, 50)[0] > 200, "left button is lit");
    assert!(layer.pixel(200, 50)[0] < 100, "right button is not");

    let alone = diagram::Plan {
        keyboard: false,
        ..diagram::Plan::FULL
    };
    let layer = Layer::diagram(std::ptr::null(), alone, 1.).unwrap();
    layer.begin(false).unwrap();
    layer.devices(&devices, 0);
    layer.end().unwrap();
    assert_eq!(layer.width, alone.width());
    assert!(layer.pixel(22, 50)[0] > 200, "a lone mouse shows its click");
    assert_eq!(layer.pixel(3, 130)[3], 0, "and no keyboard panel");
}
