#![cfg(windows)]
use shotoshot_app::capture_desktop;
use windows::Win32::{
    System::Threading::*,
    UI::{HiDpi::*, WindowsAndMessaging::*},
};

#[test]
fn desktop_capture_uses_virtual_dimensions_and_releases_gdi_objects() {
    unsafe {
        let previous = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let before = GetGuiResources(GetCurrentProcess(), GR_GDIOBJECTS);
        for _ in 0..3 {
            let image = capture_desktop().unwrap();
            assert_eq!(image.width, GetSystemMetrics(SM_CXVIRTUALSCREEN) as u32);
            assert_eq!(image.height, GetSystemMetrics(SM_CYVIRTUALSCREEN) as u32);
            assert_eq!(image.left, GetSystemMetrics(SM_XVIRTUALSCREEN));
            assert_eq!(image.top, GetSystemMetrics(SM_YVIRTUALSCREEN));
            assert_eq!(
                image.pixels.len(),
                image.width as usize * image.height as usize * 4
            );
            assert!(image.pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
        }
        assert_eq!(GetGuiResources(GetCurrentProcess(), GR_GDIOBJECTS), before);
        SetThreadDpiAwarenessContext(previous);
    }
}
