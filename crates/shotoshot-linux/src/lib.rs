//! ROLE: Linux platform layer: capture, window, clipboard, files and resident process.
//! OWNER: The `shotoshot` binary on Linux.
#![cfg(target_os = "linux")]
mod clipboard;
mod editor;
mod files;
mod keymap;
mod monitors;
mod notice;
mod png_io;
mod portal;
mod resident;
mod settings;
mod x11;
use std::{error::Error, path::PathBuf};

/// Runs the command line; a failure the user did not ask for (not a cancelled portal
/// dialog) is also shown in a message window, since a shortcut has no terminal.
pub fn run() -> Result<(), Box<dyn Error>> {
    // A file name need not be UTF-8: only the switches are read as text.
    let args: Vec<String> = std::env::args_os()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let result = dispatch(&args);
    let unattended = args.iter().any(|a| {
        a == clipboard::SERVE
            || a == notice::ARG
            || a == notice::CONFIRMATION
            || a.starts_with("--linux-")
    });
    if let Err(error) = &result
        && !unattended
        && !portal::cancelled(error.as_ref())
    {
        notice::report(&format!("Shotoshot: {error}"));
    }
    result
}

fn dispatch(args: &[String]) -> Result<(), Box<dyn Error>> {
    if args.iter().any(|a| a == clipboard::SERVE) {
        return clipboard::serve();
    }
    for (kind, duration) in [
        (notice::ARG, notice::DURATION),
        (notice::CONFIRMATION, notice::BRIEF),
    ] {
        if let Some(i) = args.iter().position(|a| a == kind) {
            let message = args
                .get(i + 1)
                .ok_or("a message window needs its message")?;
            return notice::show(message, duration);
        }
    }
    if let Some(i) = args.iter().position(|a| a == "--linux-diagnostic") {
        let out = args
            .get(i + 1)
            .map(PathBuf::from)
            .ok_or("--linux-diagnostic needs an output folder")?;
        return diagnostic(&out);
    }
    if args.iter().any(|a| a == "--linux-x11-probe") {
        // Diagnostic of the X11 path only: size, not content.
        let image = x11::screenshot()?;
        println!("x11: {}x{}", image.width, image.height);
        return Ok(());
    }
    if args.iter().any(|a| a == "--instant") {
        return instant();
    }
    settings::migrate_former_name();
    // A first argument that is not a switch is a document: what the desktop passes when a
    // `.shoto` file is opened. It is edited in this process, resident or not.
    if let Some(path) = std::env::args_os()
        .nth(1)
        .filter(|a| !a.to_string_lossy().starts_with("--"))
    {
        let probe = args.iter().any(|a| a == editor::PROBE);
        return editor::open(std::path::Path::new(&path), probe);
    }
    // `shotoshot` starts the resident; with one already running, or with `--capture`
    // (the command to bind to a desktop shortcut), it captures now.
    if !args.iter().any(|a| a == "--capture") && resident::run()? {
        return Ok(());
    }
    editor::run(capture()?, None)
}

/// Saves the whole desktop as the next "Shot NNN.png" on the Desktop, without editing.
fn instant() -> Result<(), Box<dyn Error>> {
    let image = capture()?;
    let folder = editor::user_dir("XDG_DESKTOP_DIR", "Desktop");
    files::save_numbered(&png_io::encode(&image)?, &folder, false)?;
    Ok(())
}

/// Scripted end-to-end check on a real desktop: every step writes evidence into `out`.
fn diagnostic(out: &std::path::Path) -> Result<(), Box<dyn Error>> {
    std::fs::create_dir_all(out)?;
    let started = std::time::Instant::now();
    let image = capture()?;
    println!(
        "capture: {}x{} in {} ms",
        image.width,
        image.height,
        started.elapsed().as_millis()
    );
    png_io::save(&image, &out.join("capture.png"))?;
    editor::run(image, Some(out.to_path_buf()))?;
    // Both message windows must open on the real desktop too, from their own process.
    for (kind, message) in [
        (notice::ARG, "Shotoshot diagnostic: message window"),
        (notice::CONFIRMATION, editor::SAVED_EDITABLE),
    ] {
        let status = notice::spawn(kind, message)?.wait()?;
        if !status.success() {
            return Err(format!("message window {kind} failed: {status}").into());
        }
    }
    println!("notice: failure and confirmation shown");
    Ok(())
}

/// The whole desktop. X11 sessions capture the root window directly and fall back to the
/// portal (desktops with one); Wayland only allows the portal (XWayland would see black).
fn capture() -> Result<shotoshot_ui::Image, Box<dyn Error>> {
    let x11_session = std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t == "x11")
        || (std::env::var_os("WAYLAND_DISPLAY").is_none() && std::env::var_os("DISPLAY").is_some());
    if x11_session {
        x11::screenshot().or_else(|error| portal::screenshot().map_err(|_| error))
    } else {
        portal::screenshot()
    }
}
