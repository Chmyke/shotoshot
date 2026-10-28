use super::*;
use shotoshot_core::{Rect, Tool};

#[test]
fn capture_shortcut_cannot_steal_a_suspended_shape_shortcut() {
    let owner = crate::resources::OwnedWindow(unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("STATIC"),
            w!("Shortcut conflict test"),
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
    let mut app = App {
        settings: Settings::default(),
        session: None,
        setup: true,
        setup_ui: setup::Setup {
            pending: Some((1, 0x52)),
            ..Default::default()
        },
        hotkey: crate::hotkey::Hotkey::new(owner.0),
        instant: instant::InstantCapture::new(owner.0),
        board_hotkey: crate::hotkey::Hotkey::with_id(owner.0, 5),
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
        pointer_window: None,
    };
    app.setup_activate(owner.0, 2).unwrap();
    assert!(
        app.setup,
        "a rejected combination must leave configuration open"
    );
    assert!(!app.setup_ui.error.is_empty());
    assert_eq!(app.settings.key, 0);
    assert!(!app.hotkey.active());
    assert!(
        !app.save_shape_shortcut(owner.0, 0, 0, 0x41).unwrap(),
        "plain letters cannot be global drawing shortcuts"
    );
    // A shape shortcut only hides the window, so closing can be observed without quitting.
    app.setup_ui = setup::Setup::change(setup::ShortcutTarget::Arrow, Some((1, 0x46)));
    let size = app.setup_ui.layout.size;
    paint_image(size.x as u32, size.y as u32, |dc| {
        app.setup_ui.draw(
            dc,
            RECT {
                right: size.x as i32,
                bottom: size.y as i32,
                ..Default::default()
            },
        )
    })
    .unwrap();
    assert!(app.setup_ui.painter.is_some());
    app.close_setup(owner.0).unwrap();
    assert!(!app.setup);
    assert!(
        app.setup_ui.painter.is_none(),
        "the resident keeps no renderer once the shortcut window is closed"
    );
}

#[test]
fn diagnostics_and_the_document_probe_never_write_the_settings() {
    let persists =
        |args: &[&str]| runtime::persists_preferences(args.iter().map(|a| a.to_string()));
    assert!(persists(&["shotoshot.exe"]), "the resident");
    assert!(persists(&["shotoshot.exe", "--board"]), "a board session");
    assert!(
        persists(&["shotoshot.exe", "notes.shoto"]),
        "a reopened document"
    );
    assert!(
        !persists(&["shotoshot.exe", "notes.shoto", "--document-probe"]),
        "the document diagnostic's child saves without touching the user's settings"
    );
    assert!(!persists(&["shotoshot.exe", "--diagnostic"]));
    assert!(!persists(&["shotoshot.exe", "--first-frame-diagnostic"]));
}

#[test]
fn the_uninstaller_runs_exactly_the_restores_the_app_offers() {
    let script = installer_script();
    let section = script
        .split("[UninstallRun]")
        .nth(1)
        .and_then(|rest| rest.split("\n[").next())
        .expect("an [UninstallRun] section");
    let run: Vec<&str> = section
        .lines()
        .filter(|line| line.starts_with("Filename:"))
        .map(|line| {
            assert!(
                line.contains("Flags: runhidden waituntilterminated"),
                "hidden, and finished before the files go: {line}"
            );
            let parameters = line.split("Parameters: \"").nth(1).expect(line);
            parameters.split('"').next().unwrap()
        })
        .collect();
    assert_eq!(run, runtime::UNINSTALL_RESTORES);
}

fn installer_script() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packaging/shotoshot.iss"
    ))
    .unwrap()
}

/// Updating is downloading the installer again and running it (2026-10-15): no task page to
/// answer, no desktop shortcut offered. Only a pre-0.2.0 one is carried over. The one question
/// is the language (2026-10-17), on a page of its own.
#[test]
fn the_installer_asks_only_the_language_and_offers_no_desktop_shortcut() {
    let script = installer_script();
    assert!(!script.contains("[Tasks]"), "a task page is back");
    let code = script.split("\n[Code]").nth(1).expect("a [Code] section");
    let pages: Vec<&str> = code
        .lines()
        .filter(|line| {
            line.split("Create")
                .skip(1)
                .any(|rest| rest.split('(').next().is_some_and(|f| f.ends_with("Page")))
        })
        .collect();
    assert_eq!(pages.len(), 1, "exactly one page of our own: {pages:?}");
    assert!(pages[0].contains("CreateInputOptionPage"), "{}", pages[0]);
    assert!(script.contains("\nShowLanguageDialog=no\n"));
    // Every language of the wizard has its messages in the generated include, and the other
    // way round (shotoshot-text, example `inno`).
    let languages = script
        .split("\n[Languages]\n")
        .nth(1)
        .and_then(|rest| rest.split("\n[").next())
        .expect("a [Languages] section");
    let mut names: Vec<&str> = languages
        .lines()
        .filter_map(|line| line.strip_prefix("Name: \""))
        .map(|rest| rest.split('"').next().unwrap())
        .collect();
    names.sort_unstable();
    let messages = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packaging/languages.isi"
    ))
    .unwrap();
    let mut translated: Vec<&str> = messages
        .lines()
        .filter_map(|line| line.split_once(".LanguagePageTitle="))
        .map(|(name, _)| name)
        .collect();
    translated.sort_unstable();
    assert_eq!(names, translated);
    assert!(script.contains("#include \"languages.isi\""));
    let desktop: Vec<&str> = script
        .lines()
        .filter(|line| line.starts_with("Name: \"{autodesktop}"))
        .collect();
    assert_eq!(
        desktop,
        [
            "Name: \"{autodesktop}\\{#AppName}\"; Filename: \"{app}\\{#AppExeName}\"; Check: KeepsOldDesktopLink"
        ]
    );
}

/// The language page is prefilled from `settings.txt`, read by the installer's own Pascal
/// code: it must accept the format `Settings::encode` writes and read its language line.
#[test]
fn installer_reads_the_language_line_of_the_current_settings_format() {
    let script = installer_script();
    let constant = |name: &str| -> String {
        let line = script
            .lines()
            .find(|line| line.trim_start().starts_with(&format!("{name} = ")))
            .unwrap_or_else(|| panic!("constant {name} in [Code]"));
        line.split(" = ")
            .nth(1)
            .unwrap()
            .trim_end_matches(';')
            .trim_matches('\'')
            .to_string()
    };
    let defaults = Settings::default().encode();
    let header = defaults.lines().next().unwrap();
    let (prefix, version) = header.rsplit_once('-').unwrap();
    assert_eq!(constant("SettingsHeader"), format!("{prefix}-"));
    let known = constant("KnownSettingsVersions");
    assert!(
        known.split(',').any(|v| v == version),
        "version {version} of Settings::encode is not in KnownSettingsVersions = '{known}'"
    );
    let czech = Settings {
        language: shotoshot_text::LanguageChoice::Fixed(shotoshot_text::Language::Cs),
        ..Default::default()
    }
    .encode();
    let changed: Vec<usize> = defaults
        .lines()
        .zip(czech.lines())
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(index, _)| index)
        .collect();
    assert_eq!(changed.len(), 1, "the language is one line");
    assert_eq!(constant("SettingsLanguageLine"), changed[0].to_string());
}

#[test]
fn a_restore_run_by_the_uninstaller_fails_without_a_message_box() {
    let on_stderr =
        |args: &[&str]| runtime::reports_errors_on_stderr(args.iter().map(|a| a.to_string()));
    for restore in runtime::UNINSTALL_RESTORES {
        assert!(
            on_stderr(&["shotoshot.exe", restore]),
            "{restore} would block a silent uninstall on a message box"
        );
    }
    assert!(on_stderr(&["shotoshot.exe", "--board-diagnostic"]));
    assert!(
        on_stderr(&["shotoshot.exe", "--forget-lens-page", "lens-1-2.html"]),
        "the page forgetter runs after its editor has gone"
    );
    assert!(
        !on_stderr(&["shotoshot.exe"]),
        "the resident shows its errors"
    );
    assert!(
        !on_stderr(&["shotoshot.exe", "--board"]),
        "so does a session"
    );
    assert!(
        !on_stderr(&["shotoshot.exe", "--board-bake"]),
        "and a bake nobody watches"
    );
}

fn hidden_window() -> crate::resources::OwnedWindow {
    crate::resources::OwnedWindow(unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("STATIC"),
            w!("Board shortcut test"),
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
/// A resident with its shortcut window open, without the user's settings.
fn resident(owner: HWND) -> App {
    App {
        settings: Settings::default(),
        session: None,
        setup: true,
        setup_ui: setup::Setup::default(),
        hotkey: crate::hotkey::Hotkey::new(owner),
        instant: instant::InstantCapture::new(owner),
        board_hotkey: crate::hotkey::Hotkey::with_id(owner, 5),
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
        pointer_window: None,
    }
}

#[test]
fn the_board_shortcut_refuses_the_capture_combination() {
    let owner = hidden_window();
    let mut app = resident(owner.0);
    app.settings.modifiers = 1;
    app.settings.key = 66;
    app.setup_ui.target = setup::ShortcutTarget::Board;
    app.save_board_shortcut(owner.0, 1, 66).unwrap();
    assert!(app.setup_ui.error.contains("already used"));
    app.setup_ui.error.clear();
    app.save_board_shortcut(owner.0, 0, 66).unwrap();
    assert!(
        app.setup_ui.error.contains("Ctrl or Alt"),
        "{}",
        app.setup_ui.error
    );
    assert!(app.setup && !app.board_hotkey.active());
    assert_eq!(
        app.settings.board_shortcut,
        Settings::default().board_shortcut
    );
    // Ctrl+Shift+Alt+F22: free on a test desktop, where the installed Shotoshot may hold Alt+B.
    app.setup_ui.error.clear();
    app.save_board_shortcut(owner.0, 7, 0x85).unwrap();
    assert!(app.setup_ui.error.is_empty(), "{}", app.setup_ui.error);
    assert!(app.board_hotkey.active());
    assert_eq!(
        app.settings.board_shortcut,
        crate::settings::Shortcut {
            modifiers: 7,
            key: 0x85
        }
    );
    assert!(
        !app.setup,
        "the window closes and the resident keeps running"
    );
}

#[test]
fn the_other_shortcuts_refuse_the_board_combination() {
    let owner = hidden_window();
    let mut app = resident(owner.0);
    let board = app.settings.board_shortcut;
    app.setup_ui.pending = Some((board.modifiers, board.key));
    app.setup_activate(owner.0, 2).unwrap();
    assert!(app.setup_ui.error.contains("board"), "capture");
    assert!(!app.hotkey.active());
    app.setup_ui.error.clear();
    app.save_instant_shortcut(owner.0, board.modifiers, board.key)
        .unwrap();
    assert!(app.setup_ui.error.contains("board"), "instant");
    assert!(!app.instant.hotkey.active());
    app.setup_ui.error.clear();
    assert!(
        !app.save_shape_shortcut(owner.0, 0, board.modifiers, board.key)
            .unwrap()
    );
    assert!(app.setup_ui.error.contains("board"), "shape");
    assert!(app.setup, "every refusal leaves the window open");
}

#[test]
fn language_change_restarts_the_prepared_editor() {
    use shotoshot_text::{Key, Language, LanguageChoice, tr_in};
    let owner = hidden_window();
    let mut app = resident(owner.0);
    app.worker = Some(editor_process::tests::idle_editor());
    // English: the process-wide language other tests read stays the same.
    let choice = tray_menu::on_language_choice(152).unwrap();
    assert_eq!(choice, LanguageChoice::Fixed(Language::En));
    app.choose_language(owner.0, choice).unwrap();
    assert_eq!(app.settings.language, choice, "the choice is kept");
    let worker = app.worker.as_ref().unwrap();
    assert!(
        editor_process::tests::ends(worker),
        "the prepared editor kept its old texts"
    );
    assert!(worker.replenish(1), "its exit prepares the next editor");
    let tip = String::from_utf16_lossy(&app.tray.szTip);
    assert_eq!(
        tip.trim_end_matches('\0'),
        tr_in(Language::En, Key::TrayTooltip)
    );
}

#[test]
fn presentation_preferences_reapply_the_language_only_after_reading_the_settings_back() {
    use shotoshot_text::{Language, LanguageChoice};
    let owner = hidden_window();
    let mut app = resident(owner.0);
    // A diagnostic resident: nothing is read back, so its language stays as it is.
    app.settings.language = LanguageChoice::Fixed(Language::De);
    let mut preferences = app.settings.presentation.clone();
    preferences.stroke_width = 9;
    preferences.input_locked = true;
    preferences.input_offset = Some([12, 34]);
    let reloaded = app.store_presentation_preferences(&preferences).unwrap();
    assert!(
        !reloaded,
        "nothing was read back: the language is not applied"
    );
    assert_eq!(app.settings.presentation.stroke_width, 9);
    assert!(app.settings.presentation.input_locked);
    assert_eq!(app.settings.presentation.input_offset, Some([12, 34]));
    assert_eq!(app.settings.language, LanguageChoice::Fixed(Language::De));
}

#[test]
fn tray_tip_fits() {
    use shotoshot_text::{Key, Language, tr_in};
    for language in Language::ALL {
        let text = tr_in(language, Key::TrayTooltip);
        assert!(text.encode_utf16().count() <= 127, "{}", language.tag());
        let tip = tray_tip(text);
        assert_eq!(
            String::from_utf16_lossy(&tip[..text.encode_utf16().count()]),
            text
        );
        assert!(
            tip[text.encode_utf16().count()..]
                .iter()
                .all(|&unit| unit == 0)
        );
    }
    assert_eq!(tray_tip(&"x".repeat(300))[127], 0, "always terminated");
    assert_eq!(
        tr_in(Language::En, Key::TrayTooltip),
        "Shotoshot: capture and annotate"
    );
}

#[test]
fn the_menu_stays_open_for_settings_and_the_icon_closes_it() {
    let owner = hidden_window();
    let mut app = resident(owner.0);
    app.setup = false;
    let open = |app: &App| {
        tray_menu::Popup::open(
            owner.0,
            RECT::default(),
            app.menu_values().unwrap(),
            POINT {
                x: -32000,
                y: -32000,
            },
            false,
        )
        .unwrap()
    };
    app.menu = Some(open(&app));
    app.menu_command(owner.0, 121, true).unwrap();
    assert_eq!(app.settings.presentation.duration_ms, 1000);
    assert!(app.menu.is_some(), "a setting keeps the menu open");
    // A click on the icon closes the open menu rather than opening another.
    app.tray_click(owner.0).unwrap();
    assert!(app.menu.is_none());
    // A press on the icon closed it by taking the foreground: its release opens nothing.
    app.menu = Some(open(&app));
    let root = app.menu.as_ref().unwrap().root();
    app.menu_closed(root, true);
    assert!(app.menu.is_none());
    app.tray_click(owner.0).unwrap();
    assert!(app.menu.is_none() && app.menu_closed_on_icon.is_none());
    // A late notice from an earlier menu leaves the open one alone.
    app.menu = Some(open(&app));
    app.menu_closed(root, false);
    assert!(app.menu.is_some());
    // A language is a setting too: the menu stays, in the chosen language. English keeps
    // the process-wide language other tests read. An idle editor stands for the prepared one,
    // so that restarting it launches no real process.
    app.worker = Some(editor_process::tests::idle_editor());
    app.menu_command(owner.0, 152, true).unwrap();
    assert_eq!(
        app.settings.language,
        shotoshot_text::LanguageChoice::Fixed(shotoshot_text::Language::En)
    );
    assert!(app.menu.is_some(), "a language keeps the menu open");
    // A command closes the menu before it runs.
    app.menu_command(owner.0, 0, false).unwrap();
    assert!(app.menu.is_none());
}

#[test]
fn stop_presentation_is_handled_even_while_a_menu_borrows_the_app() {
    unsafe {
        let instance = GetModuleHandleW(None).unwrap();
        assert_ne!(
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance.into(),
                lpszClassName: w!("Shotoshot.PresentationRoutingTest"),
                ..Default::default()
            }),
            0
        );
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
            pointer_window: None,
            setup_ui: setup::Setup::default(),
        }));
        let window = crate::resources::OwnedWindow(
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                w!("Shotoshot.PresentationRoutingTest"),
                w!("Test"),
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
        let p = crate::presentation::Presentation::new(window.0, Default::default()).unwrap();
        p.borrow_mut().enabled.shapes = true;
        state.borrow_mut().presentation = Some(p);
        let app_borrow = state.borrow_mut();
        SendMessageW(
            window.0,
            editor_process::READY,
            Some(WPARAM(7)),
            Some(LPARAM(9)),
        );
        SendMessageW(
            window.0,
            editor_process::EXITED,
            Some(WPARAM(7)),
            Some(LPARAM(0)),
        );
        let pending = editor_events::take(window.0);
        assert_eq!(
            pending.len(),
            2,
            "the modal menu swallowed the editor lifecycle"
        );
        assert_eq!(pending[0].1, editor_process::READY);
        assert_eq!(pending[1].1, editor_process::EXITED);
        SendMessageW(
            window.0,
            WM_HOTKEY,
            Some(WPARAM(crate::presentation::HOTKEY_BASE as usize)),
            None,
        );
        assert!(
            !app_borrow
                .presentation
                .as_ref()
                .unwrap()
                .borrow()
                .enabled
                .any(),
            "the modal menu swallowed the stop shortcut"
        );
    }
}

#[test]
fn entering_the_live_eyedropper_samples_without_mouse_movement() {
    let _com = ComApartment::new().unwrap();
    // A painted desktop: the real screen is neither drawn on nor read.
    let screen = crate::image::Screen::painted(-200, -100, 640, 420).unwrap();
    screen.fill(
        RECT {
            left: 100,
            top: 100,
            right: 300,
            bottom: 260,
        },
        0x005b3511,
    );
    let image = screen.blank().unwrap();
    let point = Point {
        x: 120. - image.left as f64,
        y: 120. - image.top as f64,
    };
    let mut session = crate::session::Preparation::cold(image.width, image.height, true)
        .unwrap()
        .activate(image, true)
        .unwrap();
    session.native.screen = screen;
    session.capture.pointer = point;
    assert_eq!(session.capture.sample_color(point), [0, 0, 0]);
    unsafe {
        let owner = crate::resources::OwnedWindow(
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                w!("STATIC"),
                w!("Shotoshot sample test"),
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
        let mut app = App {
            settings: Settings::default(),
            session: Some(session),
            setup: false,
            setup_ui: setup::Setup::default(),
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
            toast: None,
            google_search: None,
            first_frame: None,
            presentation: None,
            board_rebake_forced: false,
            menu: None,
            menu_closed_on_icon: None,
            pointer_window: None,
        };
        app.action(owner.0, Action::Eyedropper).unwrap();
        assert_eq!(
            app.session.as_ref().unwrap().capture.sample_color(point),
            [0x11, 0x35, 0x5b],
            "entering the pipette must show fresh pixels before the next mouse move"
        );
    }
}

#[test]
fn shortcut_recorder_accepts_pause_single_keys_and_combinations_without_saving() {
    let mut setup = super::setup::Setup::default();
    for (mods, key) in [(0, 19), (0, 65), (0, 96), (0, 27), (6, 19), (3, 65)] {
        setup.recording = true;
        setup.record(mods, key);
        assert_eq!(setup.pending, Some((mods, key)));
        assert!(!setup.recording);
    }
    setup.recording = true;
    setup.record(2, 17);
    assert!(setup.recording, "a modifier waits for its companion key");
    let english = |mods, key| super::setup::key_label_in(mods, key, shotoshot_text::Language::En);
    assert_eq!(english(0, 19), "Pause");
    assert_eq!(english(6, 19), "Ctrl + Shift + Pause");
}

#[test]
fn first_run_proposes_print_screen_and_explains_the_snipping_tool() {
    let first = super::setup::Setup::first_run();
    assert_eq!(first.pending, Some((0, 0x2C)));
    assert!(!first.recording, "Save confirms the proposal directly");
    assert_eq!(first.focus, 2);
    assert_eq!(
        super::setup::key_label_in(0, 0x2C, shotoshot_text::Language::En),
        "Print Screen"
    );
    let held = Error::from_hresult(HRESULT::from_win32(ERROR_HOTKEY_ALREADY_REGISTERED.0));
    let error = |mods, key| {
        super::setup::print_screen_error(mods, key, &held, shotoshot_text::Language::En)
    };
    assert!(error(0, 0x2C).is_some());
    assert!(error(4, 0x2C).is_none());
    assert!(error(0, 19).is_none());
}

fn packed(x: i32, y: i32) -> LPARAM {
    LPARAM(((y as u16 as u32) << 16 | x as u16 as u32) as isize)
}
unsafe fn send(hwnd: HWND, msg: u32, w: usize, x: i32, y: i32) {
    unsafe {
        // Synthetic messages have no physical keyboard queue: isolate this test thread's
        // modifier state from keys the user happens to hold. No input is sent to other apps.
        let mut previous = [0u8; 256];
        GetKeyboardState(&mut previous).unwrap();
        SetKeyboardState(&[0; 256]).unwrap();
        SendMessageW(hwnd, msg, Some(WPARAM(w)), Some(packed(x, y)));
        SetKeyboardState(&previous).unwrap();
    }
}

#[test]
fn native_window_messages_edit_export_and_recover_after_failure() {
    let _com = ComApartment::new().unwrap();
    unsafe {
        let instance = GetModuleHandleW(None).unwrap();
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: w!("Shotoshot.Test"),
            ..Default::default()
        };
        assert_ne!(RegisterClassW(&class), 0);
        let state = Box::new(RefCell::new(App {
            settings: Settings::default(),
            session: Some(Session::new(Image::solid(1000, 700, [230, 230, 230, 255])).unwrap()),
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
            pointer_window: None,
            setup_ui: setup::Setup::default(),
        }));
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("Shotoshot.Test"),
            w!("Test"),
            WS_POPUP,
            -32000,
            -32000,
            1000,
            700,
            None,
            None,
            Some(instance.into()),
            Some((&*state as *const RefCell<App>).cast()),
        )
        .unwrap();
        {
            let mut app = state.borrow_mut();
            app.setup = true;
            app.setup_ui.recording = true;
        }
        send(hwnd, WM_KEYDOWN, 19, 0, 0);
        assert_eq!(state.borrow().setup_ui.pending, Some((0, 19)));
        assert_eq!(state.borrow().settings.key, 0, "recording is not saving");
        state.borrow_mut().setup_ui.recording = true;
        send(hwnd, WM_KEYDOWN, 13, 0, 0);
        SendMessageW(hwnd, WM_KEYDOWN, Some(WPARAM(13)), Some(LPARAM(1 << 30)));
        assert!(state.borrow().setup, "key auto-repeat must not save");
        state.borrow_mut().setup_ui.recording = true;
        send(hwnd, WM_KEYUP, 44, 0, 0);
        assert_eq!(state.borrow().setup_ui.pending, Some((0, 44)));
        state.borrow_mut().setup_ui.recording = true;
        send(hwnd, WM_HOTKEY, 1, 6, 19);
        assert_eq!(state.borrow().setup_ui.pending, Some((6, 19)));
        state.borrow_mut().setup = false;
        send(hwnd, WM_LBUTTONDOWN, 0, 100, 100);
        send(hwnd, WM_MOUSEMOVE, 0, 850, 500);
        send(hwnd, WM_LBUTTONUP, 0, 850, 500);
        assert_eq!(
            state
                .borrow()
                .session
                .as_ref()
                .unwrap()
                .capture
                .region
                .unwrap()
                .size(),
            Point { x: 750., y: 400. }
        );
        send(hwnd, WM_KEYDOWN, 82, 0, 0);
        send(hwnd, WM_MOUSEWHEEL, 120usize << 16, 150, 200);
        assert!(
            state.borrow().pending_brush.is_some(),
            "a wheel change must remain pending until the end of the burst"
        );
        let first_deadline = state.borrow().pending_brush.unwrap().1;
        send(hwnd, WM_MOUSEWHEEL, 120usize << 16, 150, 200);
        let pending = state.borrow().pending_brush.unwrap();
        assert_eq!(pending.0.stroke_width, 5);
        assert!(pending.1 >= first_deadline);
        send(hwnd, WM_TIMER, BRUSH_TIMER, 0, 0);
        assert_eq!(
            state.borrow().pending_brush,
            Some(pending),
            "an early timer must not flush a newer wheel burst"
        );
        let path = std::env::temp_dir().join(format!("shotoshot-wheel-{}.txt", std::process::id()));
        let resident = Settings {
            key: 77,
            live_capture: true,
            ..Default::default()
        };
        resident
            .save_to(&path, None, None, shotoshot_core::settings::rename)
            .unwrap();
        assert_eq!(Settings::load_from(&path).unwrap().capture_stroke_width, 3);
        {
            let mut app = state.borrow_mut();
            let error = app
                .flush_brush_with(|_| Err("disk unavailable".into()))
                .unwrap_err();
            assert!(error.to_string().contains("disk unavailable"));
            assert_eq!(
                app.pending_brush,
                Some(pending),
                "failed writes remain dirty for the close retry"
            );
            app.flush_brush_with(|brush| {
                Settings::save_editor_to(
                    &path,
                    None,
                    Some(brush.stroke_width),
                    Some(brush.eraser_size),
                    None,
                    shotoshot_core::settings::rename,
                )
            })
            .unwrap();
            assert!(app.pending_brush.is_none());
            app.flush_brush_with(|_| panic!("an already flushed burst must not be written again"))
                .unwrap();
        }
        let stored = Settings::load_from(&path).unwrap();
        assert_eq!(stored.capture_stroke_width, 5);
        assert_eq!(stored.key, 77);
        assert!(
            stored.live_capture,
            "an editor must not overwrite the resident's new Live setting"
        );
        std::fs::remove_file(path).unwrap();
        send(hwnd, WM_LBUTTONDOWN, 0, 150, 200);
        send(hwnd, WM_MOUSEMOVE, 0, 350, 400);
        send(hwnd, WM_LBUTTONUP, 0, 350, 400);
        assert_eq!(
            state
                .borrow()
                .session
                .as_ref()
                .unwrap()
                .capture
                .editor
                .objects()
                .len(),
            1
        );
        send(hwnd, WM_KEYDOWN, 65, 0, 0);
        send(hwnd, WM_LBUTTONDOWN, 0, 700, 300);
        send(hwnd, WM_MOUSEMOVE, 0, 340, 300);
        send(hwnd, WM_LBUTTONUP, 0, 340, 300);
        assert_eq!(
            state
                .borrow()
                .session
                .as_ref()
                .unwrap()
                .capture
                .editor
                .objects()
                .len(),
            2
        );
        {
            let mut app = state.borrow_mut();
            let s = app.session.as_mut().unwrap();
            s.capture
                .editor
                .select_only(s.capture.editor.objects()[0].id);
            s.action(HWND::default(), Action::CustomColor);
        }
        let before_palette = state
            .borrow()
            .session
            .as_ref()
            .unwrap()
            .capture
            .editor
            .objects()
            .to_vec();
        send(hwnd, WM_KEYDOWN, 46, 0, 0);
        send(hwnd, WM_KEYDOWN, 39, 0, 0);
        assert_eq!(
            state
                .borrow()
                .session
                .as_ref()
                .unwrap()
                .capture
                .editor
                .objects(),
            before_palette
        );
        send(hwnd, WM_KEYDOWN, 27, 0, 0);
        {
            let mut app = state.borrow_mut();
            app.session.as_mut().unwrap().capture.selecting = true;
        }
        send(hwnd, WM_LBUTTONDOWN, 0, 50, 50);
        send(hwnd, WM_MOUSEMOVE, 0, 300, 300);
        send(hwnd, WM_KEYDOWN, 27, 0, 0);
        assert!(state.borrow().session.is_some());
        assert_eq!(
            state
                .borrow()
                .session
                .as_ref()
                .unwrap()
                .capture
                .region
                .unwrap()
                .min,
            Point { x: 100., y: 100. }
        );
        assert_eq!(
            state
                .borrow()
                .session
                .as_ref()
                .unwrap()
                .capture
                .editor
                .objects()
                .len(),
            2
        );
        send(hwnd, WM_KEYDOWN, 84, 0, 0);
        send(hwnd, WM_LBUTTONDOWN, 0, 420, 200);
        state
            .borrow_mut()
            .session
            .as_mut()
            .unwrap()
            .native
            .text
            .as_mut()
            .unwrap()
            .drag_anchor = Some(0);
        send(hwnd, WM_CAPTURECHANGED, 0, 0, 0);
        assert!(
            state
                .borrow()
                .session
                .as_ref()
                .unwrap()
                .native
                .text
                .as_ref()
                .unwrap()
                .drag_anchor
                .is_none()
        );
        let color_button = state
            .borrow()
            .session
            .as_ref()
            .unwrap()
            .capture
            .buttons()
            .into_iter()
            .find(|b| matches!(b.action, Action::Color([80, 153, 244])))
            .unwrap()
            .bounds
            .center();
        send(
            hwnd,
            WM_LBUTTONDOWN,
            0,
            color_button.x as i32,
            color_button.y as i32,
        );
        assert_eq!(
            state
                .borrow()
                .session
                .as_ref()
                .unwrap()
                .native
                .text
                .as_ref()
                .unwrap()
                .color,
            [80, 153, 244]
        );
        let wheel_button = state
            .borrow()
            .session
            .as_ref()
            .unwrap()
            .capture
            .buttons()
            .into_iter()
            .find(|b| matches!(b.action, Action::CustomColor))
            .unwrap()
            .bounds
            .center();
        send(
            hwnd,
            WM_LBUTTONDOWN,
            0,
            wheel_button.x as i32,
            wheel_button.y as i32,
        );
        assert_eq!(
            GetFocus(),
            hwnd,
            "picker receives keys while text stays editable"
        );
        send(hwnd, WM_KEYDOWN, 27, 0, 0);
        assert_eq!(
            GetFocus(),
            state
                .borrow()
                .session
                .as_ref()
                .unwrap()
                .native
                .text
                .as_ref()
                .unwrap()
                .window
        );
        // Alt+Tab must give the other app the foreground without rebuilding the session.
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, WS_EX_APPWINDOW.0 as isize);
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
        .unwrap();
        let (input, objects, region, image) = {
            let app = state.borrow();
            let s = app.session.as_ref().unwrap();
            (
                s.native.text.as_ref().unwrap().window,
                s.capture.editor.objects().to_vec(),
                s.capture.region,
                s.capture.image.pixels.as_ptr(),
            )
        };
        SetWindowTextW(input, w!("Draft é 日本")).unwrap();
        SendMessageW(
            input,
            windows::Win32::UI::Controls::EM_SETSEL,
            Some(WPARAM(2)),
            Some(LPARAM(7)),
        );
        let other = crate::resources::OwnedWindow(
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!("Other application"),
                WS_POPUP,
                -32000,
                -32000,
                100,
                100,
                None,
                None,
                None,
                None,
            )
            .unwrap(),
        );
        for _ in 0..3 {
            SetWindowPos(
                other.0,
                Some(HWND_TOP),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )
            .unwrap();
            // A file dialog can run a nested message loop while App is borrowed.
            let borrowed = state.borrow_mut();
            send(hwnd, WM_ACTIVATEAPP, 0, 0, 0);
            assert_eq!(
                GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST.0,
                0,
                "the editor still covers the other application"
            );
            let mut below = GetWindow(other.0, GW_HWNDNEXT).unwrap_or_default();
            while !below.0.is_null() && below != hwnd {
                below = GetWindow(below, GW_HWNDNEXT).unwrap_or_default();
            }
            assert_eq!(
                below, hwnd,
                "the capture must be below the other application"
            );
            send(hwnd, WM_ACTIVATEAPP, 1, 0, 0);
            assert_ne!(
                GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST.0,
                0
            );
            drop(borrowed);
            SetFocus(Some(hwnd)).unwrap();
            send(hwnd, WM_ACTIVATE, WA_ACTIVE as usize, 0, 0);
            assert_eq!(GetFocus(), input);
            let mut text = [0u16; 32];
            let length = GetWindowTextW(input, &mut text) as usize;
            assert_eq!(String::from_utf16(&text[..length]).unwrap(), "Draft é 日本");
            assert_eq!(
                SendMessageW(input, windows::Win32::UI::Controls::EM_GETSEL, None, None).0,
                2 | (7 << 16)
            );
            let app = state.borrow();
            let s = app.session.as_ref().unwrap();
            assert_eq!(s.capture.editor.objects(), objects);
            assert_eq!(s.capture.region, region);
            assert_eq!(s.capture.image.pixels.as_ptr(), image);
        }
        {
            let mut app = state.borrow_mut();
            let s = app.session.as_mut().unwrap();
            let input = s.native.text.as_ref().unwrap().window;
            SetWindowTextW(input, w!("Unicode é 日本 🎮\nSecond line")).unwrap();
            let preview = paint_image(1000, 700, |dc| s.paint(dc)).unwrap();
            let preview_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/shotoshot-inline-text.png");
            save_png(&preview, &preview_path).unwrap();
            s.native.finish_text(&mut s.capture, hwnd, true);
            assert_eq!(s.capture.editor.objects().len(), 3);
            s.capture.editor.undo();
            assert_eq!(s.capture.editor.objects().len(), 2);
            s.capture.editor.redo();
            let rendered = s.export().unwrap();
            assert_eq!((rendered.width, rendered.height), (750, 400));
            let before = s.capture.editor.objects().to_vec();
            s.rebuild_renderer().unwrap();
            assert_eq!(s.export().unwrap().pixels, rendered.pixels);
            let bad = std::env::temp_dir()
                .join(format!("shotoshot-missing-{}", std::process::id()))
                .join("image.png");
            assert!(save_png(&rendered, &bad).is_err());
            assert_eq!(s.capture.editor.objects(), before);
            let path =
                std::env::temp_dir().join(format!("shotoshot-window-{}.png", std::process::id()));
            save_png(&rendered, &path).unwrap();
            assert_eq!(load_png(&path).unwrap().pixels, rendered.pixels);
            std::fs::remove_file(path).unwrap();
        }
        // Replay the actual Windows wheel path with the left button held.
        {
            let mut app = state.borrow_mut();
            let mut s = Session::new(Image::solid(500, 400, [25, 25, 25, 255])).unwrap();
            s.capture.region = Rect::new(Point::default(), Point { x: 500., y: 400. });
            s.capture.selecting = false;
            s.capture.toolbar_visible = false;
            s.capture.editor.tool = Tool::Pen;
            s.capture.editor.style.width = 4.;
            app.session = Some(s);
        }
        send(hwnd, WM_LBUTTONDOWN, 1, 30, 150);
        send(hwnd, WM_MOUSEMOVE, 1, 120, 150);
        for _ in 0..4 {
            send(hwnd, WM_MOUSEWHEEL, (120usize << 16) | 1, 120, 150);
        }
        send(hwnd, WM_MOUSEMOVE, 1, 220, 150);
        {
            let app = state.borrow();
            let s = app.session.as_ref().unwrap();
            assert!(s.capture.editor.is_drawing());
            let runs = shotoshot_core::stroke_runs(&s.capture.editor.objects()[0]);
            assert_eq!(runs.iter().map(|r| r.1).collect::<Vec<_>>(), vec![4., 8.]);
            let image = s.export().unwrap();
            assert_eq!(
                image.pixels[(153 * 500 + 70) * 4 + 2],
                25,
                "old stroke got wider"
            );
            assert!(
                image.pixels[(153 * 500 + 170) * 4 + 2] > 100,
                "new stroke missing or too thin"
            );
            save_png(
                &image,
                &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../target/shotoshot-variable-width.png"),
            )
            .unwrap();
        }
        send(hwnd, WM_LBUTTONUP, 0, 220, 150);
        {
            let mut app = state.borrow_mut();
            let s = app.session.as_mut().unwrap();
            let before = s.capture.editor.objects().to_vec();
            s.capture.editor.undo();
            assert!(s.capture.editor.objects().is_empty());
            s.capture.editor.redo();
            assert_eq!(s.capture.editor.objects(), before);
        }
        {
            let mut app = state.borrow_mut();
            app.session.as_mut().unwrap().capture.editor.style.width = 12.;
            app.session.as_mut().unwrap().capture.editor.eraser_size = 40.;
            app.persist_brush(hwnd).unwrap();
            assert_eq!(
                app.pending_brush.map(|(brush, _)| brush.eraser_size),
                Some(40),
                "the eraser size is remembered with the stroke width"
            );
            app.close_session(hwnd);
            assert!(
                app.pending_brush.is_none(),
                "normal close flushes before its timer expires"
            );
            assert!(app.session.is_none());
        }
        DestroyWindow(hwnd).unwrap();
        UnregisterClassW(w!("Shotoshot.Test"), Some(instance.into())).unwrap();
    }
}

#[test]
fn toolbar_gaps_do_not_draw_on_the_image() {
    let _com = ComApartment::new().unwrap();
    let mut s = Session::new(Image::solid(1000, 700, [230, 230, 230, 255])).unwrap();
    s.capture.region = Rect::new(Point::default(), Point { x: 1000., y: 700. });
    s.capture.selecting = false;
    let first = s.capture.buttons()[0].bounds;
    let gap = Point {
        x: first.max.x + 1.,
        y: first.min.y + 10.,
    };
    s.mouse_down(HWND::default(), gap).unwrap();
    s.mouse_move(HWND::default(), gap + Point { x: 90., y: 90. });
    s.mouse_up(HWND::default(), gap + Point { x: 90., y: 90. });
    assert!(
        s.capture.editor.objects().is_empty(),
        "opaque toolbar must intercept its gaps"
    );
    let before = s.capture.toolbar_bounds().unwrap();
    assert!(
        !s.capture.toolbar_contains(before.center()),
        "space between panels remains drawable"
    );
    let edge = s.capture.buttons()[0].bounds.min - Point { x: 2., y: 2. };
    s.mouse_down(HWND::default(), edge).unwrap();
    s.mouse_move(HWND::default(), edge + Point { x: -100., y: -100. });
    s.mouse_up(HWND::default(), edge + Point { x: -100., y: -100. });
    assert_eq!(
        s.capture.toolbar_bounds().unwrap(),
        before,
        "panels cannot be dragged"
    );
    let moved = before;
    s.capture.toolbar_visible = false;
    assert!(s.capture.buttons().is_empty());
    let p = moved.center();
    s.mouse_down(HWND::default(), p).unwrap();
    s.mouse_up(HWND::default(), p + Point { x: 60., y: 60. });
    assert_eq!(
        s.capture.editor.objects().len(),
        1,
        "hidden tools leave the canvas accessible"
    );
}

#[test]
fn selecting_region_corner_resizes_without_losing_annotations() {
    let _com = ComApartment::new().unwrap();
    let mut s = Session::new(Image::solid(1000, 700, [0, 0, 0, 255])).unwrap();
    s.capture.region = Rect::new(Point { x: 100., y: 100. }, Point { x: 600., y: 400. });
    s.capture.selecting = false;
    s.capture
        .editor
        .add_text(Point { x: 200., y: 200. }, "Keep me".into());
    s.mouse_down(HWND::default(), Point { x: 100., y: 100. })
        .unwrap();
    s.mouse_up(HWND::default(), Point { x: 50., y: 60. });
    assert_eq!(s.capture.region.unwrap().min, Point { x: 50., y: 60. });
    assert_eq!(s.capture.editor.objects().len(), 1);
}

#[test]
fn region_edges_resize_one_axis_and_picker_reads_outside_capture() {
    let _com = ComApartment::new().unwrap();
    let mut s = Session::new(Image::solid(1000, 700, [30, 20, 10, 255])).unwrap();
    s.capture.region = Rect::new(Point { x: 100., y: 100. }, Point { x: 600., y: 400. });
    s.capture.selecting = false;
    s.mouse_down(HWND::default(), Point { x: 350., y: 100. })
        .unwrap();
    s.mouse_up(HWND::default(), Point { x: 400., y: 50. });
    assert_eq!(s.capture.region.unwrap().min, Point { x: 100., y: 50. });
    assert_eq!(s.capture.region.unwrap().max, Point { x: 600., y: 400. });
    s.action(HWND::default(), Action::Eyedropper);
    let effect = s
        .mouse_down(HWND::default(), Point { x: 900., y: 650. })
        .unwrap();
    assert_eq!(effect, Some(Action::CopyColor([10, 20, 30])));
    assert_eq!(s.capture.editor.style.color, [10, 20, 30]);
    assert!(!s.capture.eyedropper);
    let before = s.capture.editor.style.color;
    let clean_export = s.export().unwrap().pixels;
    s.capture.pointer = Point { x: 100., y: 100. };
    s.action(HWND::default(), Action::CustomColor);
    let square_center =
        s.capture.picker.as_ref().unwrap().bounds().min + Point { x: 130., y: 144. };
    s.mouse_down(HWND::default(), square_center).unwrap();
    s.mouse_up(HWND::default(), square_center);
    assert_ne!(s.capture.picker.as_ref().unwrap().color(), before);
    assert_eq!(
        s.export().unwrap().pixels,
        clean_export,
        "picker overlays never enter export"
    );
    assert!(s.cancel_gesture(HWND::default()));
    assert_eq!(
        s.capture.editor.style.color, before,
        "Escape discards pending color"
    );
    s.action(HWND::default(), Action::CustomColor);
    let square_center =
        s.capture.picker.as_ref().unwrap().bounds().min + Point { x: 130., y: 144. };
    s.mouse_down(HWND::default(), square_center).unwrap();
    s.mouse_up(HWND::default(), square_center);
    let chosen = s.capture.picker.as_ref().unwrap().color();
    assert_eq!(
        s.key(HWND::default(), 13),
        shotoshot_ui::keys::KeyOutcome::Handled
    );
    assert_eq!(
        s.capture.editor.style.color, chosen,
        "Enter keeps the wheel's colour"
    );
    assert!(s.capture.picker.is_none());
    // A click outside the panel applies the wheel's colour; the screen is the eyedropper's job.
    s.action(HWND::default(), Action::CustomColor);
    s.mouse_down(HWND::default(), square_center).unwrap();
    s.mouse_up(HWND::default(), square_center);
    let chosen = s.capture.picker.as_ref().unwrap().color();
    let effect = s
        .mouse_down(HWND::default(), Point { x: 900., y: 650. })
        .unwrap();
    assert_eq!(effect, None);
    assert_eq!(s.capture.editor.style.color, chosen);
    assert!(s.capture.picker.is_none());
}

#[test]
fn rectangle_has_four_straight_sides() {
    let _com = ComApartment::new().unwrap();
    let mut fresh = Session::new(Image::solid(400, 300, [0, 0, 0, 255])).unwrap();
    fresh.capture.editor.tool = Tool::Rectangle;
    fresh.capture.editor.begin(
        Point { x: 30., y: 30. },
        shotoshot_core::Modifiers::default(),
    );
    fresh.capture.editor.update(Point { x: 300., y: 250. });
    fresh.capture.editor.finish();
    let paths = shotoshot_core::paths(
        &fresh.capture.editor.objects()[0],
        fresh.capture.editor.objects(),
    );
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0].len(), 5);
    assert!(
        paths[0]
            .windows(2)
            .all(|p| p[0].x == p[1].x || p[0].y == p[1].y)
    );
}

#[test]
fn live_one_pixel_arrow_extends_outside_crop_but_export_stays_cropped() {
    let _com = ComApartment::new().unwrap();
    let mut s = Session::new(Image::solid(500, 400, [25, 25, 25, 255])).unwrap();
    s.capture.region = Rect::new(Point { x: 50., y: 50. }, Point { x: 250., y: 250. });
    s.capture.selecting = false;
    s.capture.toolbar_visible = false;
    s.capture.editor.tool = Tool::Arrow;
    s.capture.editor.style.width = 1.;
    s.mouse_down(HWND::default(), Point { x: 100., y: 150. })
        .unwrap();
    s.mouse_move(HWND::default(), Point { x: 350., y: 150. });
    let preview = crate::paint_image(500, 400, |dc| s.paint(dc)).unwrap();
    assert!(
        preview.pixels[(150 * 500 + 300) * 4 + 2] > 60,
        "outside arrow must remain visible during drag"
    );
    assert!(
        preview.pixels[(150 * 500 + 175) * 4 + 2] > 60,
        "one pixel arrow must appear during drag"
    );
    s.mouse_up(HWND::default(), Point { x: 350., y: 150. });
    let output = s.export().unwrap();
    assert_eq!((output.width, output.height), (200, 200));
}

#[test]
fn objects_outside_capture_can_be_reselected_moved_and_resized() {
    let _com = ComApartment::new().unwrap();
    let mut s = Session::new(Image::solid(1000, 700, [25, 25, 25, 255])).unwrap();
    let crop = Rect::new(Point { x: 100., y: 100. }, Point { x: 400., y: 350. }).unwrap();
    s.capture.region = Some(crop);
    s.capture.selecting = false;
    let empty = s.export().unwrap().pixels;
    s.capture.editor.tool = Tool::Rectangle;
    s.mouse_down(HWND::default(), Point { x: 150., y: 150. })
        .unwrap();
    s.mouse_up(HWND::default(), Point { x: 230., y: 230. });
    s.action(HWND::default(), Action::Tool(Tool::Select));
    s.mouse_down(HWND::default(), Point { x: 190., y: 150. })
        .unwrap();
    s.mouse_up(HWND::default(), Point { x: 650., y: 150. });
    s.capture.editor.selected.clear();
    s.mouse_down(HWND::default(), Point { x: 650., y: 150. })
        .unwrap();
    assert_eq!(
        s.capture.editor.selected.len(),
        1,
        "outside object cannot be reselected"
    );
    s.mouse_up(HWND::default(), Point { x: 650., y: 190. });
    let moved = s.capture.editor.objects()[0].bounds();
    assert_eq!(moved.min, Point { x: 610., y: 190. });
    s.mouse_down(HWND::default(), moved.max).unwrap();
    s.mouse_up(HWND::default(), Point { x: 750., y: 310. });
    assert_eq!(
        s.capture.editor.objects()[0].bounds().max,
        Point { x: 750., y: 310. }
    );
    assert_eq!(s.capture.region, Some(crop));
    assert_eq!(s.export().unwrap().pixels, empty);
    s.capture.editor.undo();
    assert_eq!(s.capture.editor.objects()[0].bounds(), moved);
    s.capture.editor.redo();
    assert_eq!(
        s.capture.editor.objects()[0].bounds().max,
        Point { x: 750., y: 310. }
    );
}

#[test]
fn history_buttons_follow_commit_undo_redo_and_new_branch() {
    let _com = ComApartment::new().unwrap();
    let mut s = Session::new(Image::solid(1000, 700, [230, 230, 230, 255])).unwrap();
    s.capture.region = Rect::new(Point::default(), Point { x: 1000., y: 700. });
    s.capture.selecting = false;
    let states = |s: &Session| {
        let buttons = s.capture.buttons();
        (
            buttons
                .iter()
                .find(|b| matches!(b.action, Action::Undo))
                .unwrap()
                .enabled,
            buttons
                .iter()
                .find(|b| matches!(b.action, Action::Redo))
                .unwrap()
                .enabled,
        )
    };
    assert_eq!(states(&s), (false, false));
    let undo = s
        .capture
        .buttons()
        .into_iter()
        .find(|b| matches!(b.action, Action::Undo))
        .unwrap();
    assert!(
        s.mouse_down(HWND::default(), undo.bounds.center())
            .unwrap()
            .is_none()
    );
    assert!(s.capture.editor.objects().is_empty());
    s.capture
        .editor
        .add_text(Point { x: 20., y: 20. }, "First".into());
    assert_eq!(states(&s), (true, false));
    s.action(HWND::default(), Action::Undo);
    assert_eq!(states(&s), (false, true));
    s.action(HWND::default(), Action::Redo);
    assert_eq!(states(&s), (true, false));
    s.action(HWND::default(), Action::Undo);
    s.capture
        .editor
        .add_text(Point { x: 30., y: 30. }, "New branch".into());
    assert_eq!(states(&s), (true, false));
}

/// A test window of its own class around an app holding `session`, as the resident creates
/// one; the caller destroys it. Preferences are never persisted.
unsafe fn app_window(class: PCWSTR, session: Session) -> (Box<RefCell<App>>, HWND) {
    unsafe {
        let instance = GetModuleHandleW(None).unwrap();
        let class_info = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: class,
            ..Default::default()
        };
        assert_ne!(RegisterClassW(&class_info), 0);
        let state = Box::new(RefCell::new(App {
            settings: Settings::default(),
            session: Some(session),
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
            pointer_window: None,
            setup_ui: setup::Setup::default(),
        }));
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class,
            w!("Test"),
            WS_POPUP,
            -32000,
            -32000,
            1000,
            700,
            None,
            None,
            Some(instance.into()),
            Some((&*state as *const RefCell<App>).cast()),
        )
        .unwrap();
        (state, hwnd)
    }
}

#[test]
fn board_window_messages_zoom_pan_and_reset_the_pointed_screen() {
    let _com = ComApartment::new().unwrap();
    unsafe {
        let screens = vec![shotoshot_core::board::Screen {
            id: "one".into(),
            bounds: Rect::new(Point::default(), Point { x: 1000., y: 700. }).unwrap(),
        }];
        let session = Session::board(
            Image::solid(1000, 700, [230, 230, 230, 255]),
            None,
            Default::default(),
            screens,
            std::env::temp_dir(),
        )
        .unwrap();
        let (state, hwnd) = app_window(w!("Shotoshot.Test.BoardView"), session);
        let views = || {
            state
                .borrow()
                .session
                .as_ref()
                .unwrap()
                .capture
                .views
                .clone()
        };
        send(hwnd, WM_MOUSEMOVE, 0, 100, 100);
        SendMessageW(
            hwnd,
            WM_MOUSEWHEEL,
            Some(WPARAM((120usize << 16) | events::MK_CONTROL)),
            Some(packed(100, 100)),
        );
        assert_eq!(views()[0].zoom, 1.25, "Ctrl+wheel zooms the pointed screen");
        assert_eq!(
            state
                .borrow()
                .session
                .as_ref()
                .unwrap()
                .capture
                .editor
                .style
                .width,
            3.,
            "the stroke width is not what Ctrl+wheel changes"
        );
        let origin = views()[0].origin;
        send(hwnd, WM_MBUTTONDOWN, 0, 500, 300);
        send(hwnd, WM_MOUSEMOVE, 0, 450, 280);
        send(hwnd, WM_MBUTTONUP, 0, 450, 280);
        assert_ne!(views()[0].origin, origin, "the middle button pans");
        assert!(
            !state.borrow().session.as_ref().unwrap().capture.panning(),
            "released"
        );
        // Ctrl+0 through the capture: the keyboard state of a synthetic message is isolated.
        {
            let mut app = state.borrow_mut();
            let (capture, mut host) = app.session.as_mut().unwrap().host(hwnd);
            let outcome = capture.key(
                &mut host,
                48,
                shotoshot_core::Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
            );
            assert_eq!(outcome, shotoshot_ui::keys::KeyOutcome::Handled);
        }
        assert_eq!(views()[0].zoom, 1., "Ctrl+0 resets");
        state.borrow_mut().session = None;
        DestroyWindow(hwnd).unwrap();
        UnregisterClassW(
            w!("Shotoshot.Test.BoardView"),
            Some(GetModuleHandleW(None).unwrap().into()),
        )
        .unwrap();
    }
}

#[test]
fn windows_ending_its_session_writes_the_board_and_keeps_it_open() {
    let _com = ComApartment::new().unwrap();
    let folder =
        std::env::temp_dir().join(format!("shotoshot-board-endsession-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    unsafe {
        let screens = vec![shotoshot_core::board::Screen {
            id: "one".into(),
            bounds: Rect::new(Point::default(), Point { x: 1000., y: 700. }).unwrap(),
        }];
        let session = Session::board(
            Image::solid(1000, 700, [230, 230, 230, 255]),
            None,
            Default::default(),
            screens,
            folder.clone(),
        )
        .unwrap();
        let (state, hwnd) = app_window(w!("Shotoshot.Test.EndSession"), session);
        // A note drawn, then a text being typed: both must reach the document.
        send(hwnd, WM_KEYDOWN, 82, 0, 0);
        send(hwnd, WM_LBUTTONDOWN, 0, 100, 100);
        send(hwnd, WM_MOUSEMOVE, 0, 300, 200);
        send(hwnd, WM_LBUTTONUP, 0, 300, 200);
        send(hwnd, WM_KEYDOWN, 84, 0, 0);
        send(hwnd, WM_LBUTTONDOWN, 0, 500, 500);
        send(hwnd, WM_LBUTTONUP, 0, 500, 500);
        let input = state
            .borrow()
            .session
            .as_ref()
            .unwrap()
            .native
            .text
            .as_ref()
            .expect("the text field is open")
            .window;
        SetWindowTextW(input, w!("Buy milk")).unwrap();
        let answer = SendMessageW(hwnd, WM_QUERYENDSESSION, Some(WPARAM(0)), Some(LPARAM(0)));
        assert_eq!(answer.0, 1, "Windows may go on");
        assert!(state.borrow().session.is_some(), "the session stays open");
        let board = crate::board::bake::read_board(&folder).unwrap();
        let scene = &board.screens[0].scene;
        assert_eq!(scene.objects.len(), 2);
        assert!(scene.objects.iter().any(
            |o| matches!(&o.shape, shotoshot_core::Shape::Text { text, .. } if text == "Buy milk")
        ));
        state.borrow_mut().session = None;
        DestroyWindow(hwnd).unwrap();
        UnregisterClassW(
            w!("Shotoshot.Test.EndSession"),
            Some(GetModuleHandleW(None).unwrap().into()),
        )
        .unwrap();
    }
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_google_search_and_the_capture_shortcut_keep_the_capture_open() {
    let _com = ComApartment::new().unwrap();
    unsafe {
        let mut session = Session::new(Image::solid(1000, 700, [230, 230, 230, 255])).unwrap();
        session.capture.region = Rect::new(Point { x: 100., y: 100. }, Point { x: 600., y: 400. });
        session.capture.selecting = false;
        session
            .capture
            .editor
            .add_text(Point { x: 200., y: 200. }, "Keep me".into());
        let (state, hwnd) = app_window(w!("Shotoshot.Test.GoogleKeeps"), session);
        let (objects, image) = {
            let app = state.borrow();
            let s = app.session.as_ref().unwrap();
            (
                s.capture.editor.objects().to_vec(),
                s.capture.image.pixels.as_ptr(),
            )
        };
        assert_eq!(objects.len(), 1);
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(Ok("lens-page.html".to_string())).unwrap();
        {
            let mut app = state.borrow_mut();
            app.google_search = Some(rx);
            app.session.as_mut().unwrap().capture.searching = true;
        }
        let mut opened = None;
        state.borrow_mut().finish_google_search(hwnd, |_, page| {
            opened = Some(page.to_path_buf());
            true
        });
        assert_eq!(
            opened.as_deref(),
            Some(std::path::Path::new("lens-page.html"))
        );
        let keeps = |why: &str| {
            let app = state.borrow();
            let s = app.session.as_ref().expect(why);
            assert!(!s.capture.searching, "{why}");
            assert_eq!(s.capture.editor.objects(), objects, "{why}");
            assert_eq!(s.capture.image.pixels.as_ptr(), image, "{why}");
        };
        keeps("the capture stays open behind the browser");
        assert!(state.borrow().google_search.is_none());
        // The resident forwards the capture shortcut to a capture still open: it comes back.
        send(hwnd, CAPTURE_MESSAGE, 0, 0, 0);
        keeps("the shortcut brings the same capture back");
        state.borrow_mut().session = None;
        DestroyWindow(hwnd).unwrap();
        UnregisterClassW(
            w!("Shotoshot.Test.GoogleKeeps"),
            Some(GetModuleHandleW(None).unwrap().into()),
        )
        .unwrap();
    }
}

#[test]
fn space_pan_modifier_lets_go_when_the_window_loses_the_pointer_or_the_foreground() {
    let _com = ComApartment::new().unwrap();
    unsafe {
        let screens = vec![shotoshot_core::board::Screen {
            id: "one".into(),
            bounds: Rect::new(Point::default(), Point { x: 1000., y: 700. }).unwrap(),
        }];
        let session = Session::board(
            Image::solid(1000, 700, [230, 230, 230, 255]),
            None,
            Default::default(),
            screens,
            std::env::temp_dir(),
        )
        .unwrap();
        let (state, hwnd) = app_window(w!("Shotoshot.Test.SpacePan"), session);
        let held = || {
            state
                .borrow()
                .session
                .as_ref()
                .unwrap()
                .capture
                .space_held()
        };
        send(hwnd, WM_KEYDOWN, 32, 0, 0);
        assert!(held(), "Space held");
        send(hwnd, WM_CAPTURECHANGED, 0, 0, 0);
        assert!(!held(), "the pointer went elsewhere: Space is forgotten");
        send(hwnd, WM_KEYDOWN, 32, 0, 0);
        assert!(held());
        send(hwnd, WM_ACTIVATE, WA_INACTIVE as usize, 0, 0);
        assert!(
            !held(),
            "another window came to the front: Space is forgotten"
        );
        state.borrow_mut().session = None;
        DestroyWindow(hwnd).unwrap();
        UnregisterClassW(
            w!("Shotoshot.Test.SpacePan"),
            Some(GetModuleHandleW(None).unwrap().into()),
        )
        .unwrap();
    }
}

#[test]
fn a_closing_capture_tells_its_owner_the_region_to_offer_next() {
    let _com = ComApartment::new().unwrap();
    unsafe {
        let owner = crate::resources::OwnedWindow(
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                w!("STATIC"),
                w!("Region owner"),
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
        let mut session = Session::new(Image::solid(1000, 700, [0, 0, 0, 255])).unwrap();
        session.capture.region = Rect::new(Point { x: 40., y: 50. }, Point { x: 400., y: 300. });
        session.capture.selecting = false;
        let (state, hwnd) = app_window(w!("Shotoshot.Test.RegionRecall"), session);
        state.borrow_mut().notification_owner = Some(owner.0);
        state.borrow_mut().close_session(hwnd);
        let mut message = MSG::default();
        assert!(
            PeekMessageW(
                &mut message,
                Some(owner.0),
                recall::REGION,
                recall::REGION,
                PM_REMOVE
            )
            .as_bool()
        );
        assert_eq!(message.wParam.0 as u32, GetCurrentProcessId());
        let region = recall::unpack(message.lParam).unwrap();
        assert_eq!(
            (region.left, region.top, region.right, region.bottom),
            (40, 50, 400, 300)
        );
        let _ = DestroyWindow(hwnd);
    }
}
