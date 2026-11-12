use super::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;

fn index(id: u32) -> usize {
    ROOT.iter().position(|row| row.id == id).unwrap()
}
fn owner() -> crate::resources::OwnedWindow {
    crate::resources::OwnedWindow(unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("STATIC"),
            w!("Menu owner test"),
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
fn hidden(owner: HWND) -> Popup {
    Popup::open(
        owner,
        RECT::default(),
        Values::default(),
        POINT {
            x: -32000,
            y: -32000,
        },
        false,
    )
    .unwrap()
}
/// The messages the menu posted to `owner`, oldest first.
fn posted(owner: HWND) -> Vec<(u32, usize, isize)> {
    let mut found = Vec::new();
    let mut message = MSG::default();
    unsafe {
        while PeekMessageW(&mut message, Some(owner), COMMAND, EDIT, PM_REMOVE).as_bool() {
            found.push((message.message, message.wParam.0, message.lParam.0));
        }
    }
    found
}
unsafe fn click(window: HWND, column: &Column, row: usize) {
    unsafe { click_at(window, column, row, 20) }
}
unsafe fn click_at(window: HWND, column: &Column, row: usize, x: i32) {
    let y = (column.top(row) + column.top(row + 1)) / 2;
    let at = LPARAM(((y as isize) << 16) | x as isize);
    unsafe {
        SendMessageW(window, WM_MOUSEMOVE, None, Some(at));
        SendMessageW(window, WM_LBUTTONUP, None, Some(at));
    }
}

fn edit(id: u32, op: EditOp) -> (u32, usize, isize) {
    (EDIT, id as usize, op.pack())
}

#[test]
fn an_edit_survives_its_message() {
    for op in [
        EditOp::Begin,
        EditOp::KeyDown(0x41),
        EditOp::KeyUp(VK_SNAPSHOT.0 as u32),
        EditOp::Default,
        EditOp::Cancel,
        EditOp::Undo,
    ] {
        assert_eq!(EditOp::unpack(op.pack()), Some(op));
    }
}

#[test]
fn a_click_on_the_keys_changes_them_and_on_the_label_runs_the_command() {
    let owner = owner();
    let popup = hidden(owner.0);
    let (width, root) = (popup.model.borrow().column.width, popup.root());
    let column = |popup: &Popup| Column::new(ROOT, &popup.model.borrow().values, 1.).unwrap();
    unsafe { click_at(root, &column(&popup), index(1), width - 20) };
    assert_eq!(posted(owner.0), [edit(1, EditOp::Begin)]);
    // A shape row too: its keys change them, its label draws the shape.
    unsafe { click_at(root, &column(&popup), index(171), width - 20) };
    assert_eq!(posted(owner.0), [edit(171, EditOp::Begin)]);
    unsafe { click(root, &column(&popup), index(1)) };
    assert_eq!(posted(owner.0), [(COMMAND, 1, 0)]);
    let popup = hidden(owner.0);
    unsafe { click(popup.root(), &column(&popup), index(171)) };
    assert_eq!(posted(owner.0), [(COMMAND, 171, 0)]);
}

#[test]
fn the_pointer_lights_the_label_or_the_keys_of_a_shortcut_row() {
    let owner = owner();
    let popup = hidden(owner.0);
    let (width, root) = (popup.model.borrow().column.width, popup.root());
    let column = Column::new(ROOT, &popup.model.borrow().values, 1.).unwrap();
    let rest = |id: u32, x: i32| {
        let y = (column.top(index(id)) + column.top(index(id) + 1)) / 2;
        unsafe {
            SendMessageW(
                root,
                WM_MOUSEMOVE,
                None,
                Some(LPARAM(((y as isize) << 16) | x as isize)),
            );
        }
        let model = popup.model.borrow();
        (model.selected, model.view.pointer)
    };
    assert_eq!(rest(1, 20), (Some(index(1)), Some((1, false))));
    assert_eq!(rest(1, width - 20), (Some(index(1)), Some((1, true))));
    assert_eq!(rest(172, width - 20), (Some(index(172)), Some((172, true))));
    // Other rows light whole, and so does the keyboard.
    assert_eq!(rest(170, width - 20), (Some(index(170)), None));
    rest(8, width - 20);
    unsafe {
        SendMessageW(root, WM_KEYDOWN, Some(WPARAM(VK_DOWN.0 as usize)), None);
    }
    assert_eq!(popup.model.borrow().view.pointer, None);
}

#[test]
fn a_listening_row_takes_every_key_and_escape_keeps_the_menu() {
    let owner = owner();
    let popup = hidden(owner.0);
    let root = popup.root();
    let size = |hwnd| {
        let mut rect = RECT::default();
        unsafe { GetWindowRect(hwnd, &mut rect).unwrap() };
        rect
    };
    let before = size(root);
    popup.set_editing(Some(RowEdit {
        row: ShortcutRow::Capture,
        held: Vec::new(),
        error: String::new(),
    }));
    assert_eq!(
        size(root),
        before,
        "listening never moves or resizes the menu"
    );
    let key = |msg: u32, vk: VIRTUAL_KEY, repeat: bool| unsafe {
        SendMessageW(
            root,
            msg,
            Some(WPARAM(vk.0 as usize)),
            Some(LPARAM(if repeat { 1 << 30 } else { 1 })),
        )
    };
    key(WM_SYSKEYDOWN, VK_A, false);
    key(WM_KEYDOWN, VK_A, true);
    key(WM_SYSKEYUP, VK_A, false);
    key(WM_KEYDOWN, VK_BACK, false);
    key(WM_KEYDOWN, VK_DOWN, false);
    key(WM_KEYDOWN, VK_ESCAPE, false);
    assert_eq!(
        posted(owner.0),
        [
            edit(1, EditOp::KeyDown(VK_A.0 as u32)),
            edit(1, EditOp::KeyUp(VK_A.0 as u32)),
            edit(1, EditOp::Default),
            edit(1, EditOp::KeyDown(VK_DOWN.0 as u32)),
            edit(1, EditOp::Cancel),
        ],
        "nothing closes the menu"
    );
    assert_eq!(popup.model.borrow().selected, None);
    // A click on another row leaves the listening one as it was, and runs nothing.
    let column = Column::new(ROOT, &popup.model.borrow().values, 1.).unwrap();
    unsafe { click(root, &column, index(3)) };
    assert_eq!(posted(owner.0), [edit(1, EditOp::Cancel)]);
    assert_eq!(size(root), before);
    // Done listening: the keys walk the menu again.
    popup.set_editing(None);
    key(WM_KEYDOWN, VK_DOWN, false);
    assert!(posted(owner.0).is_empty());
    assert!(popup.model.borrow().selected.is_some());
}

#[test]
fn settings_stay_open_and_commands_close() {
    for row in ROOT {
        let expected = match row.id {
            // Switches: the menu stays open to set several in a row.
            101..=104 | 107 | 9 => Effect::Apply(row.id),
            // Each opens or runs something else; a shape row records its shortcut.
            1 | 3 | 7 | 8 | 10 | 170..=173 => Effect::Run(row.id),
            _ => Effect::None,
        };
        assert_eq!(effect(row), expected, "{}", label_in(row, Language::En));
    }
}

#[test]
fn arrow_keys_skip_titles_and_wrap() {
    assert_eq!(step(ROOT, None, true), Some(0));
    assert_eq!(step(ROOT, None, false), Some(ROOT.len() - 1));
    // From the last board row down to the first switch, over the title.
    assert_eq!(step(ROOT, Some(index(10)), true), Some(index(101)));
    assert_eq!(step(ROOT, Some(index(101)), false), Some(index(10)));
    // Through the shape shortcuts, then to the settings over the separator, and around.
    assert_eq!(step(ROOT, Some(index(102)), true), Some(index(171)));
    assert_eq!(step(ROOT, Some(index(173)), true), Some(index(170)));
    assert_eq!(step(ROOT, Some(index(170)), true), Some(index(3)));
    assert_eq!(step(ROOT, Some(index(3)), true), Some(0));
    assert_eq!(step(&[], None, true), None);
    let all_inert = &ROOT[index(90)..=index(90)];
    assert_eq!(step(all_inert, None, true), None);
}

#[test]
fn only_a_press_on_the_icon_counts_as_closing_it() {
    let icon = RECT {
        left: 100,
        top: 200,
        right: 124,
        bottom: 224,
    };
    let inside = POINT { x: 110, y: 210 };
    assert!(pressed_on_icon(icon, inside, true));
    assert!(
        !pressed_on_icon(icon, inside, false),
        "the keyboard or a release"
    );
    assert!(!pressed_on_icon(icon, POINT { x: 124, y: 210 }, true));
    assert!(!pressed_on_icon(RECT::default(), POINT::default(), true));
}

#[test]
fn rows_are_found_under_the_pointer_but_titles_never() {
    let _open = Open;
    let column = Column::new(ROOT, &Values::default(), 2.).unwrap();
    assert_eq!(column.top(0), 8, "padding above the first row");
    assert_eq!(
        column.height(),
        8 + ROOT
            .iter()
            .map(|row| (height(row) * 2.).round() as i32)
            .sum::<i32>()
            + 8
    );
    assert_eq!(column.at(0), None, "padding");
    assert_eq!(column.at(column.top(0)), Some(0));
    assert_eq!(column.at(column.top(1) - 1), Some(0));
    assert_eq!(column.at(column.top(index(90)) + 4), None, "title");
    assert_eq!(column.at(column.top(index(3))), Some(index(3)));
    assert_eq!(column.at(column.height() - 1), None, "padding");
}

#[test]
fn a_setting_keeps_the_menu_open_and_a_command_closes_it() {
    let owner = owner();
    let popup = hidden(owner.0);
    unsafe {
        let _ = ShowWindow(popup.root(), SW_SHOWNOACTIVATE);
        let root = popup.model.borrow().column.tops.clone();
        let column = Column {
            rows: ROOT,
            tops: root,
            width: 0,
        };
        click(popup.root(), &column, index(101));
        assert_eq!(posted(owner.0), [(COMMAND, 101, 1)]);
        assert!(IsWindowVisible(popup.root()).as_bool());
        click(popup.root(), &column, index(90));
        assert_eq!(posted(owner.0), [], "a title does nothing");
        click(popup.root(), &column, index(170));
        assert_eq!(posted(owner.0), [(COMMAND, 170, 0)]);
        assert!(!IsWindowVisible(popup.root()).as_bool());
    }
}

#[test]
fn the_keyboard_walks_the_menu_and_escape_closes_it() {
    let owner = owner();
    let popup = hidden(owner.0);
    let key = |vk: VIRTUAL_KEY| unsafe {
        SendMessageW(popup.root(), WM_KEYDOWN, Some(WPARAM(vk.0 as usize)), None);
    };
    // Down to the first switch, which Space turns on without closing.
    for _ in 0..6 {
        key(VK_DOWN);
    }
    assert_eq!(popup.model.borrow().selected, Some(index(101)));
    key(VK_SPACE);
    assert_eq!(posted(owner.0), [(COMMAND, 101, 1)]);
    // Past the other switches and the shape shortcuts, to the settings: Enter opens them.
    for _ in 0..8 {
        key(VK_DOWN);
    }
    assert_eq!(popup.model.borrow().selected, Some(index(170)));
    key(VK_RETURN);
    assert_eq!(posted(owner.0), [(COMMAND, 170, 0)]);
    key(VK_ESCAPE);
    assert_eq!(posted(owner.0), [(CLOSED, 0, popup.root().0 as isize)]);
}

#[test]
fn losing_the_foreground_closes_the_menu() {
    let owner = owner();
    let popup = hidden(owner.0);
    unsafe {
        SendMessageW(
            popup.root(),
            WM_ACTIVATE,
            Some(WPARAM(WA_INACTIVE as usize)),
            None,
        );
    }
    assert_eq!(posted(owner.0), [(CLOSED, 0, popup.root().0 as isize)]);
}

/// Handles what the windows of this thread receive for `ms` milliseconds, and returns
/// the notices posted to `owner` meanwhile.
fn pump(owner: HWND, ms: u64) -> Vec<(u32, usize, isize)> {
    let until = std::time::Instant::now() + std::time::Duration::from_millis(ms);
    let mut notices = Vec::new();
    let mut message = MSG::default();
    while std::time::Instant::now() < until {
        unsafe {
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                if message.hwnd == owner && (COMMAND..=CLOSED).contains(&message.message) {
                    notices.push((message.message, message.wParam.0, message.lParam.0));
                    continue;
                }
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    notices
}
fn input(inputs: &[INPUT]) {
    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    assert_eq!(sent as usize, inputs.len(), "input refused");
}

#[test]
#[ignore = "real desktop: shows the menu, takes the foreground and clicks it, run alone by check.ps1"]
fn real_desktop_the_menu_keeps_the_foreground_through_a_setting_and_closes_when_left() {
    let owner = owner();
    let mut pointer = POINT::default();
    unsafe { GetCursorPos(&mut pointer).unwrap() };
    // The last input comes from this process, which may then take the foreground, as the
    // shell lets the resident do on a click on its icon. F24 is on no keyboard.
    let key = |flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VK_F24,
                dwFlags: flags,
                ..Default::default()
            },
        },
    };
    input(&[key(KEYBD_EVENT_FLAGS(0)), key(KEYEVENTF_KEYUP)]);
    let mut work = RECT::default();
    unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some((&mut work as *mut RECT).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .unwrap();
    }
    let at = POINT {
        x: (work.left + work.right) / 2,
        y: work.top + 40,
    };
    let popup = Popup::open(owner.0, RECT::default(), Values::default(), at, true).unwrap();
    assert_eq!(pump(owner.0, 200), []);
    let root = popup.root();
    assert_eq!(
        unsafe { GetForegroundWindow() },
        root,
        "the menu has the keyboard"
    );
    // A real click on the halo switch.
    let mut bounds = RECT::default();
    unsafe { GetWindowRect(root, &mut bounds).unwrap() };
    assert_eq!((bounds.left, bounds.top), (at.x, at.y));
    let row = {
        let model = popup.model.borrow();
        let column = &model.column;
        (column.top(index(101)) + column.top(index(101) + 1)) / 2
    };
    unsafe { SetCursorPos(bounds.left + 30, bounds.top + row).unwrap() };
    assert_eq!(pump(owner.0, 50), []);
    let button = |flags| INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dwFlags: flags,
                ..Default::default()
            },
        },
    };
    input(&[button(MOUSEEVENTF_LEFTDOWN), button(MOUSEEVENTF_LEFTUP)]);
    let clicked = pump(owner.0, 200);
    unsafe { SetCursorPos(pointer.x, pointer.y).unwrap() };
    assert_eq!(clicked, [(COMMAND, 101, 1)]);
    assert!(unsafe { IsWindowVisible(root) }.as_bool());
    assert_eq!(
        unsafe { GetForegroundWindow() },
        root,
        "still in front after the click"
    );
    // Another window taking the foreground stands for a click elsewhere.
    let other = crate::resources::OwnedWindow(unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("STATIC"),
            w!("Elsewhere"),
            WS_POPUP | WS_VISIBLE,
            work.left,
            work.top,
            1,
            1,
            None,
            None,
            None,
            None,
        )
        .unwrap()
    });
    unsafe {
        let _ = SetForegroundWindow(other.0);
    }
    assert_eq!(pump(owner.0, 100), [(CLOSED, 0, root.0 as isize)]);
    assert!(!unsafe { IsWindowVisible(root) }.as_bool());
}

#[test]
fn a_closed_popup_keeps_no_renderer_or_window() {
    let owner = owner();
    let root = {
        let popup = hidden(owner.0);
        assert!(PAINTER.with(|painter| painter.borrow().is_some()));
        popup.root()
    };
    assert!(PAINTER.with(|painter| painter.borrow().is_none()));
    assert!(!unsafe { IsWindow(Some(root)) }.as_bool());
}
