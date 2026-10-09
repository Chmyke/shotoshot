#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(windows)]
fn main() {
    if let Err(error) = shotoshot_app::run() {
        if std::env::args().any(|a| {
            matches!(
                a.as_str(),
                "--diagnostic"
                    | "--render-demo"
                    | "--editor-diagnostic"
                    | "--first-frame-diagnostic"
                    | "--prepared-editor"
                    | "--warm-diagnostic"
                    | "--presentation-diagnostic"
                    | "--google-diagnostic"
                    | "--restore-print-screen"
            )
        }) {
            eprintln!("{error}");
            std::process::exit(1);
        }
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
                None,
                &windows::core::HSTRING::from(error.to_string()),
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
