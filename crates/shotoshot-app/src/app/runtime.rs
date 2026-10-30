//! ROLE: Initializes the resident or editor process and owns its message loop.
//! ORDER: COM and window guards outlive all callbacks and session resources.
use super::*;

/// Whether a process started with `args` may write the user's settings. Never a diagnostic
/// that uses the default settings, nor the document diagnostic's child: it stands in for
/// the user, and its Save would rewrite their real settings in this build's format.
pub(super) fn persists_preferences(mut args: impl Iterator<Item = String>) -> bool {
    !args.any(|a| {
        matches!(
            a.as_str(),
            "--diagnostic" | "--first-frame-diagnostic" | document::PROBE
        )
    })
}

const RESTORE_PRINT_SCREEN: &str = "--restore-print-screen";
const RESTORE_WALLPAPER: &str = "--restore-wallpaper";
/// What the uninstaller runs before it removes the files (`packaging/shotoshot.iss`,
/// `[UninstallRun]`), in this order: each gives back to Windows something Shotoshot took.
pub(super) const UNINSTALL_RESTORES: [&str; 2] = [RESTORE_PRINT_SCREEN, RESTORE_WALLPAPER];

/// Whether a process started with `args` has nobody to read a message box: its error goes to
/// stderr with exit code 1 (see main.rs).
pub fn reports_errors_on_stderr(mut args: impl Iterator<Item = String>) -> bool {
    args.any(|a| {
        UNINSTALL_RESTORES.contains(&a.as_str())
            || matches!(
                a.as_str(),
                "--diagnostic"
                    | "--render-demo"
                    | "--render-site-previews"
                    | "--editor-diagnostic"
                    | "--first-frame-diagnostic"
                    | "--prepared-editor"
                    | "--warm-diagnostic"
                    | "--presentation-diagnostic"
                    | "--loupe-diagnostic"
                    | "--google-diagnostic"
                    | "--document-diagnostic"
                    | "--board-diagnostic"
                    | "--board-diagnostic-set"
                    | "--document-probe"
                    | crate::google_search::FORGET_PAGE
            )
    })
}

/// Speaks the language of the saved settings in a process that shows its errors in a message
/// box without starting from the settings itself; English when they cannot be read.
fn apply_saved_language() {
    if let Ok(settings) = Settings::load() {
        crate::language::apply(&settings);
    }
}

pub fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args
        .iter()
        .position(|a| a == crate::google_search::FORGET_PAGE)
    {
        // Spawned after a Google Lens search: no window, no COM, only a delayed deletion.
        let page = args.get(i + 1).map(String::as_str).unwrap_or_default();
        return crate::google_search::forget_later(std::path::Path::new(page));
    }
    if std::env::args().any(|a| a == RESTORE_PRINT_SCREEN) {
        // Called by the uninstaller: no window, no COM, errors go to stderr (see main.rs).
        // A pre-0.2.0 backup of the Print Screen option must be found under the new name.
        crate::rename_migration::run();
        return crate::print_screen::restore();
    }
    if std::env::args().any(|a| a == "--document-diagnostic") {
        let _com = ComApartment::new()?;
        return document::probe();
    }
    let document = document::requested();
    let diagnostic = std::env::args().any(|a| a == "--diagnostic");
    // The first-frame diagnostic is a real editor, without the user's settings.
    let first_frame = std::env::args().any(|a| a == "--first-frame-diagnostic");
    let prepared_editor = std::env::args().any(|a| a == "--prepared-editor");
    // The board's notes in the editor, a process of its own like a reopened document.
    let board = std::env::args().any(|a| a == "--board");
    let editor_process = first_frame
        || prepared_editor
        || document.is_some()
        || board
        || std::env::args().any(|a| a == "--editor");
    let _com = ComApartment::new()?;
    // PITFALL: Set process DPI before creating any window or reading screen metrics.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    if std::env::args().any(|a| a == "--board-bake") {
        // Spawned by the resident: no window and no mutex of the roles below, only the
        // bake's own lock. Screen rectangles need the DPI awareness set above.
        apply_saved_language();
        return crate::board::bake::process();
    }
    if std::env::args().any(|a| a == RESTORE_WALLPAPER) {
        // Called by the uninstaller: no window, errors go to stderr (see main.rs).
        return crate::board::bake::restore();
    }
    if std::env::args().any(|a| a == "--board-clear") {
        // Spawned by the resident's Clear board: the same footing as a bake.
        apply_saved_language();
        return crate::board::clear::process();
    }
    if std::env::args().any(|a| a == "--board-diagnostic") {
        // Reads the screens and renders a synthetic board under target/: the desktop is not set.
        return crate::board::probe::run();
    }
    if std::env::args().any(|a| a == "--board-diagnostic-set") {
        // Explicit opt-in, never run by check.ps1: really sets the desktop, then gives it back.
        return crate::board::probe::run_set();
    }
    if std::env::args().any(|a| a == "--google-diagnostic") {
        // Explicit opt-in, never run by check.ps1: generated pixels only, never the desktop, sent
        // by the default browser as a real search would be. A person reads the result there.
        let image = crate::Image::solid(64, 64, [40, 90, 180, 255]);
        let page = crate::google_search::prepare(&image)?;
        if !crate::google_search::open(HWND::default(), &page) {
            return Err(Error::new(
                E_FAIL,
                "The default browser did not open the page.",
            ));
        }
        std::fs::write(
            "target/google-diagnostic.txt",
            "OPENED: synthetic search handed to the default browser; check that Google shows results.",
        )
        .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
        return Ok(());
    }
    if let Some(i) = args.iter().position(|a| a == "--render-site-previews") {
        // --render-site-previews <desktop pictures folder> <output folder>
        let (Some(backdrops), Some(out)) = (args.get(i + 1), args.get(i + 2)) else {
            return Err(Error::new(
                E_INVALIDARG,
                "usage: --render-site-previews <in> <out>",
            ));
        };
        return crate::site_previews::render(backdrops.as_ref(), out.as_ref());
    }
    if std::env::args().any(|a| a == "--render-demo") {
        crate::demo::render_demo()?;
        return setup::preview();
    }
    if std::env::args().any(|a| a == "--presentation-diagnostic") {
        return crate::presentation::probe::run();
    }
    if std::env::args().any(|a| a == "--loupe-diagnostic") {
        return crate::presentation::loupe_probe::run();
    }
    if std::env::args().any(|a| a == "--editor-diagnostic") {
        return crate::diagnostics::editor_probe();
    }
    if std::env::args().any(|a| a == "--warm-diagnostic") {
        return super::warm_probe::run();
    }
    if std::env::args().any(|a| a == "--hotkey-diagnostic") {
        // Explicit probe: keys must be free on this interactive desktop.
        // No simulated input, settings writes or changes to another app's hotkeys.
        for (mods, key) in [(0, 19), (6, 19), (0, 65), (0, 96), (0, 27), (3, 65)] {
            let mut hotkey = crate::hotkey::Hotkey::new(HWND::default());
            hotkey.replace(None, mods, key, || Ok(()))?;
        }
        return Ok(());
    }
    unsafe {
        let mutex = CreateMutexW(
            None,
            false,
            if diagnostic {
                w!("Local\\Shotoshot.Diagnostics")
            } else if first_frame {
                w!("Local\\Shotoshot.FirstFrameDiagnostic")
            } else if board {
                // Never the editor's name, as for a document; one board session at a time.
                w!("Local\\Shotoshot.Board")
            } else if document.is_some() {
                // Never the editor's name: holding it would stop the resident's next editor.
                w!("Local\\Shotoshot.Document")
            } else if editor_process {
                w!("Local\\Shotoshot.Editor")
            } else {
                w!("Local\\Shotoshot.Resident")
            },
        )?;
        let _mutex = crate::resources::OwnedMutex(mutex);
        // Reopened documents do not exclude each other.
        if GetLastError() == ERROR_ALREADY_EXISTS && document.is_none() {
            if board {
                // A board session is already open: this second request has nothing to add.
                return Ok(());
            }
            if prepared_editor {
                return Err(Error::new(E_FAIL, "An editor already exists"));
            }
            apply_saved_language();
            message(HWND::default(), &Text::new(Key::ErrorAlreadyRunning));
            return Ok(());
        }
        if !diagnostic && !editor_process {
            crate::rename_migration::run();
        }
        let mut settings = if diagnostic || first_frame {
            Settings::default()
        } else {
            let settings = Settings::load().map_err(|error| {
                // The saved choice is unknown: the error speaks the language of Windows.
                crate::language::apply(&Settings::default());
                crate::settings::unreadable(error)
            })?;
            // Resident, prepared editor, document editor and board session all start here;
            // diagnostics stay in English so that their output does not depend on the PC.
            crate::language::apply(&settings);
            settings
        };
        if !diagnostic && !editor_process {
            // The language chosen on the installer's page, if it left one, before any window.
            // Kept for this run even if it cannot be stored: the resident still starts.
            if let Some(folder) = Settings::path()?.parent() {
                crate::language::start_with_installer_choice(
                    &mut settings,
                    folder,
                    |s| s.save(),
                    crate::language::apply,
                    |detail| report(HWND::default(), &crate::settings::unsaved(detail)),
                );
            }
        }
        if first_frame && std::env::args().any(|a| a == "--live-diagnostic") {
            settings.live_capture = true;
        }
        if !diagnostic && !editor_process {
            crate::startup::set_enabled(settings.start_with_windows)?;
        }
        let instance = GetModuleHandleW(None)?;
        // Resource icons are shared for the process lifetime (LR_SHARED / LoadIconW).
        let resource = PCWSTR(std::ptr::without_provenance(1)); // MAKEINTRESOURCEW(1)
        let app_icon = LoadIconW(Some(instance.into()), resource)?;
        let tray_icon = HICON(
            LoadImageW(
                Some(instance.into()),
                resource,
                IMAGE_ICON,
                GetSystemMetrics(SM_CXSMICON),
                GetSystemMetrics(SM_CYSMICON),
                LR_SHARED,
            )?
            .0,
        );
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: w!("Shotoshot.Window"),
            hCursor: LoadCursorW(None, IDC_CROSS)?,
            hIcon: app_icon,
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 {
            return Err(Error::from_thread());
        }
        let state = Box::new(RefCell::new(App {
            setup: settings.key == 0 && !diagnostic && !editor_process,
            settings,
            session: None,
            hotkey: crate::hotkey::Hotkey::new(HWND::default()),
            instant: instant::InstantCapture::new(HWND::default()),
            board_hotkey: crate::hotkey::Hotkey::with_id(HWND::default(), 5),
            tray: NOTIFYICONDATAW::default(),
            quit_after_capture: editor_process,
            persist_preferences: persists_preferences(std::env::args()),
            pending_brush: None,
            worker: None,
            prepared: None,
            recall: Default::default(),
            notification_owner: if editor_process {
                std::env::args().find_map(|arg| {
                    arg.strip_prefix("--notification-owner=")
                        .and_then(|v| v.parse::<usize>().ok())
                        .filter(|v| *v != 0)
                        .map(|v| HWND(v as *mut _))
                })
            } else {
                None
            },
            google_search: None,
            first_frame: first_frame.then(Default::default),
            toast: None,
            presentation: None,
            board_rebake_forced: false,
            menu: None,
            menu_closed_on_icon: None,
            settings_ui: Default::default(),
            setup_ui: setup::Setup::default(),
        }));
        let hwnd = CreateWindowExW(
            if editor_process {
                WS_EX_APPWINDOW
            } else {
                WS_EX_TOOLWINDOW
            },
            w!("Shotoshot.Window"),
            w!("Shotoshot"),
            WS_POPUP,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            620,
            230,
            None,
            None,
            Some(instance.into()),
            Some((&*state as *const RefCell<App>).cast()),
        )?;
        let _window = crate::resources::OwnedWindow(hwnd);
        {
            let mut app = state.borrow_mut();
            app.hotkey = crate::hotkey::Hotkey::new(hwnd);
            app.instant = instant::InstantCapture::new(hwnd);
            app.board_hotkey = crate::hotkey::Hotkey::with_id(hwnd, 5);
            app.tray = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: 1,
                uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
                uCallbackMessage: TRAY_MESSAGE,
                hIcon: tray_icon,
                ..Default::default()
            };
            app.tray.szTip = tray_tip(shotoshot_text::tr(shotoshot_text::Key::TrayTooltip));
            if !editor_process && !Shell_NotifyIconW(NIM_ADD, &app.tray).as_bool() {
                return Err(Error::new(E_FAIL, "Cannot create the notification icon"));
            }
            // A saved bare Print Screen that Windows took back (reinstall, Windows Settings)
            // is taken again here, exactly as when it was saved.
            let free = crate::print_screen::windows_option(app.persist_preferences);
            if app.settings.key != 0 && !editor_process {
                let (mods, key) = (app.settings.modifiers, app.settings.key);
                if let Err(error) = app.hotkey.replace(free, mods, key, || Ok(())) {
                    app.setup = true;
                    let language = shotoshot_text::language();
                    let held = setup::print_screen_error(mods, key, &error, language).is_some();
                    message(
                        hwnd,
                        &Text::new(if held {
                            Key::SetupPrintScreenHeld
                        } else {
                            Key::ErrorCaptureShortcutInUse
                        }),
                    );
                }
            }
            if !diagnostic && !editor_process {
                let shortcut = app.settings.instant_shortcut;
                let reserved = app
                    .settings
                    .presentation
                    .shape_shortcuts
                    .contains(&shortcut)
                    || (app.settings.modifiers == shortcut.modifiers
                        && app.settings.key == shortcut.key)
                    || (shortcut.modifiers == 3 && shortcut.key == 27);
                if reserved
                    || app
                        .instant
                        .hotkey
                        .replace(free, shortcut.modifiers, shortcut.key, || Ok(()))
                        .is_err()
                {
                    app.toast =
                        notice::Toast::new(&Text::new(Key::NoticeInstantUnavailable), true).ok();
                }
                let shortcut = app.settings.board_shortcut;
                let reserved = app
                    .settings
                    .presentation
                    .shape_shortcuts
                    .contains(&shortcut)
                    || app.settings.instant_shortcut == shortcut
                    || (app.settings.modifiers == shortcut.modifiers
                        && app.settings.key == shortcut.key)
                    || (shortcut.modifiers == 3 && shortcut.key == 27);
                if reserved
                    || app
                        .board_hotkey
                        .replace(free, shortcut.modifiers, shortcut.key, || Ok(()))
                        .is_err()
                {
                    app.toast =
                        notice::Toast::new(&Text::new(Key::NoticeBoardUnavailable), true).ok();
                }
                if app.settings.board_visible {
                    let _ = crate::board::bake::spawn();
                }
                let presentation = crate::presentation::Presentation::new(
                    hwnd,
                    app.settings.presentation.clone(),
                )?;
                let activation = presentation.borrow_mut().start_shapes();
                app.presentation = Some(presentation);
                match activation {
                    Ok(conflicts) if !conflicts.is_empty() => {
                        let names = notice::names_in(&conflicts, shotoshot_text::language());
                        let text =
                            Text::with(Key::NoticeShortcutsUnavailable, &[("names", &names)]);
                        app.toast = notice::Toast::new(&text, true).ok();
                    }
                    Err(error) => message(hwnd, &crate::failure::brief_text(&error)),
                    _ => {}
                }
            }
            if app.setup {
                app.show_setup(hwnd)?;
            }
            if prepared_editor {
                if !std::env::args().any(|a| a == "--cold-editor") {
                    app.prepared = Some(crate::session::Preparation::new(
                        GetSystemMetrics(SM_CXVIRTUALSCREEN) as u32,
                        GetSystemMetrics(SM_CYVIRTUALSCREEN) as u32,
                        app.settings.live_capture,
                    )?);
                }
                let owner = app
                    .notification_owner
                    .ok_or_else(|| Error::new(E_INVALIDARG, "Missing editor owner"))?;
                PostMessageW(
                    Some(owner),
                    editor_process::READY,
                    WPARAM(GetCurrentProcessId() as usize),
                    LPARAM(hwnd.0 as isize),
                )?;
            } else if !editor_process && !diagnostic {
                app.prepare_editor(hwnd);
            }
        }
        if let Some(path) = &document {
            state.borrow_mut().open_document(hwnd, path)?;
        } else if board {
            state.borrow_mut().open_board(hwnd)?;
        } else if !prepared_editor && (editor_process || std::env::args().any(|a| a == "--capture"))
        {
            PostMessageW(Some(hwnd), CAPTURE_MESSAGE, WPARAM(0), LPARAM(0))?;
        }
        let mut msg = MSG::default();
        let mut probe = if diagnostic {
            Some(crate::diagnostics::Diagnostic::start()?)
        } else {
            None
        };
        loop {
            let result = GetMessageW(&mut msg, None, 0, 0).0;
            if result == -1 {
                return Err(Error::from_thread());
            }
            if result == 0 {
                if msg.wParam.0 != 0 {
                    return Err(crate::failure::visible(Key::ErrorStopped, &[]));
                }
                break;
            }
            if msg.message == crate::diagnostics::COMPLETE
                && let Some(probe) = probe.take()
            {
                probe.finish()?;
                break;
            }
            let mut consumed = false;
            if msg.message == WM_KEYDOWN
                && let Ok(mut app) = state.try_borrow_mut()
                && let Some(s) = &mut app.session
                && (s
                    .native
                    .dimension_command(&mut s.capture, hwnd, msg.wParam.0 as u32)
                    || s.text_command(hwnd, msg.wParam.0 as u32, control()))
            {
                invalidate(hwnd);
                consumed = true;
            }
            if msg.message == WM_MOUSEWHEEL
                && let Ok(mut app) = state.try_borrow_mut()
                && let Some(s) = &mut app.session
            {
                s.wheel(hwnd, (msg.wParam.0 >> 16) as u16 as i16)?;
                app.persist_brush(hwnd)?;
                invalidate(hwnd);
                consumed = true;
            }
            if !consumed {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        let mut app = state.borrow_mut();
        if let Some(worker) = app.worker.take() {
            worker.shutdown();
        }
        app.presentation = None;
        app.hotkey = crate::hotkey::Hotkey::new(HWND::default());
    }
    Ok(())
}
