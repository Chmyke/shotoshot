use windows::Win32::{
    Foundation::*,
    UI::{Shell::*, WindowsAndMessaging::*},
};

pub struct OwnedMutex(pub HANDLE);
impl Drop for OwnedMutex {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
pub struct OwnedWindow(pub HWND);
impl Drop for OwnedWindow {
    fn drop(&mut self) {
        unsafe {
            // ORDER: Clear the Rust pointer before destruction, including failure unwinding.
            SetWindowLongPtrW(self.0, GWLP_USERDATA, 0);
            let tray = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: self.0,
                uID: 1,
                ..Default::default()
            };
            let _ = Shell_NotifyIconW(NIM_DELETE, &tray);
            let _ = DestroyWindow(self.0);
        }
    }
}
