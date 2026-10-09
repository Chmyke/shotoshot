//! ROLE: Initializes the resident or editor process and owns its message loop.
//! ORDER: COM and window guards outlive all callbacks and session resources.
use super::*;

pub fn run() -> Result<()> {
    if std::env::args().any(|a| a == "--restore-print-screen") {
        // Called by the uninstaller: no window, no COM, errors go to stderr (see main.rs).
        // A pre-0.2.0 backup of the Print Screen option must be found under the new name.
        crate::rename_migration::run();
        return crate::print_screen::restore();
    }
    let diagnostic = std::env::args().any(|a| a == "--diagnostic");
    // The first-frame diagnostic is a real editor, without the user's settings.
    let first_frame = std::env::args().any(|a| a == "--first-frame-diagnostic");
    let prepared_editor = std::env::args().any(|a| a == "--prepared-editor");
    let editor_process =
        first_frame || prepared_editor || std::env::args().any(|a| a == "--editor");
    let _com = ComApartment::new()?;
    // PITFALL: Set process DPI before creating any window or reading screen metrics.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    if std::env::args().any(|a| a == "--google-diagnostic") {
        // Explicit opt-in network probe: generated pixels only, never the desktop.
        let image = crate::Image::solid(64, 64, [40, 90, 180, 255]);
        let _ = crate::google_search::upload(&image)?;
        std::fs::write(
            "target/google-diagnostic.txt",
            "PASS: synthetic PNG sent to Google Lens; validated Google result redirect.",
        )
        .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
        return Ok(());
    }
    let args: Vec<String> = std::env::args().collect();
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
            } else if editor_process {
                w!("Local\\Shotoshot.Editor")
            } else {
                w!("Local\\Shotoshot.Resident")
            },
        )?;
        let _mutex = crate::resources::OwnedMutex(mutex);
        if GetLastError() == ERROR_ALREADY_EXISTS {
            if prepared_editor {
                return Err(Error::new(E_FAIL, "An editor already exists"));
            }
            message(
                HWND::default(),
                "Shotoshot is already running. Use its notification-area icon.",
            );
            return Ok(());
        }
        if !diagnostic && !editor_process {
            crate::rename_migration::run();
        }
        let mut settings = if diagnostic || first_frame {
            Settings::default()
        } else {
            Settings::load().map_err(|e| Error::new(E_FAIL, e))?
        };
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
            tray: NOTIFYICONDATAW::default(),
            quit_after_capture: editor_process,
            persist_preferences: !diagnostic && !first_frame,
            pending_stroke_width: None,
            worker: None,
            prepared: None,
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
            app.tray = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: 1,
                uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
                uCallbackMessage: TRAY_MESSAGE,
                hIcon: tray_icon,
                ..Default::default()
            };
            for (i, c) in "Shotoshot — capture and annotate"
                .encode_utf16()
                .enumerate()
            {
                app.tray.szTip[i] = c;
            }
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
                    let text = setup::print_screen_error(mods, key, &error).unwrap_or_else(|| {
                        "The capture shortcut is already in use. Choose another combination.".into()
                    });
                    message(hwnd, &text);
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
                    app.toast = notice::Toast::new(
                        "Instant screenshot shortcut unavailable. Change it in the Shotoshot menu.",
                        true,
                    )
                    .ok();
                }
                let presentation = crate::presentation::Presentation::new(
                    hwnd,
                    app.settings.presentation.clone(),
                )?;
                let activation = presentation.borrow_mut().start_shapes();
                app.presentation = Some(presentation);
                match activation {
                    Ok(conflicts) if !conflicts.is_empty() => {
                        app.toast = notice::Toast::new(
                            &format!("Shortcut unavailable: {}", conflicts.join(", ")),
                            true,
                        )
                        .ok();
                    }
                    Err(error) => message(hwnd, &error.message()),
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
        if !prepared_editor && (editor_process || std::env::args().any(|a| a == "--capture")) {
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
                    return Err(Error::new(
                        E_FAIL,
                        "Unexpected internal failure. Shotoshot has stopped.",
                    ));
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
                app.persist_stroke_width(hwnd)?;
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
