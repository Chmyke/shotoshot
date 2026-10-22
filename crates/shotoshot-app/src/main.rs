#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(windows)]
fn main() {
    if let Err(error) = shotoshot_app::run() {
        if shotoshot_app::reports_errors_on_stderr(std::env::args()) {
            eprintln!("{}", shotoshot_app::failure::log_line(&error));
            std::process::exit(1);
        }
        // A translated sentence; an internal failure keeps its technical detail below it.
        let text = shotoshot_app::failure::text(&error);
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
                None,
                &windows::core::HSTRING::from(text.as_str()),
                windows::core::w!("Shotoshot"),
                windows::Win32::UI::WindowsAndMessaging::MB_OK
                    | windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
            );
        }
        std::process::exit(1);
    }
}
#[cfg(target_os = "linux")]
fn main() {
    if let Err(error) = shotoshot_linux::run() {
        eprintln!("shotoshot: {error}");
        std::process::exit(1);
    }
}
#[cfg(not(any(windows, target_os = "linux")))]
fn main() {
    eprintln!("shotoshot: this platform is not supported yet");
    std::process::exit(1);
}
