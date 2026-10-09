//! ROLE: Verify real clipboard ownership and DIB bytes in a private, noninteractive window station.
//! INVARIANT: Never open or modify the interactive user's clipboard.
use shotoshot_app::{
    Image, annotation_clipboard_matches, copy_annotation_marker, copy_image, copy_text,
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::BITMAPINFOHEADER,
        System::{DataExchange::*, Memory::*, StationsAndDesktops::*, Threading::*},
        UI::WindowsAndMessaging::*,
    },
    core::*,
};

struct Isolation {
    station: HWINSTA,
    desktop: Option<HDESK>,
    original_station: HWINSTA,
    original_desktop: HDESK,
}
impl Isolation {
    fn new() -> Result<Self> {
        unsafe {
            let original_station = GetProcessWindowStation()?;
            let original_desktop = GetThreadDesktop(GetCurrentThreadId())?;
            let station = CreateWindowStationW(PCWSTR::null(), 0, GENERIC_ALL.0, None)?;
            let mut guard = Self {
                station,
                desktop: None,
                original_station,
                original_desktop,
            };
            SetProcessWindowStation(station)?;
            let desktop = CreateDesktopW(
                w!("ShotoshotClipboardTest"),
                PCWSTR::null(),
                None,
                DESKTOP_CONTROL_FLAGS(0),
                GENERIC_ALL.0,
                None,
            )?;
            guard.desktop = Some(desktop);
            SetThreadDesktop(desktop)?;
            if GetProcessWindowStation()? != station {
                return Err(Error::new(E_FAIL, "Clipboard isolation failed"));
            }
            Ok(guard)
        }
    }
}
impl Drop for Isolation {
    fn drop(&mut self) {
        unsafe {
            let _ = SetProcessWindowStation(self.original_station);
            let _ = SetThreadDesktop(self.original_desktop);
            if let Some(desktop) = self.desktop {
                let _ = CloseDesktop(desktop);
            }
            let _ = CloseWindowStation(self.station);
        }
    }
}
struct Window(HWND);
impl Drop for Window {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}
fn verify() -> Result<()> {
    let _isolation = Isolation::new()?;
    unsafe {
        let owner = Window(CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("STATIC"),
            w!("Clipboard test"),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            None,
            None,
        )?);
        let mut image = Image::solid(7, 3, [10, 20, 30, 255]);
        image.pixels[0] = 99;
        image.pixels[7 * 4] = 77;
        copy_image(&image, owner.0)?;
        OpenClipboard(Some(owner.0))?;
        let result = (|| -> Result<()> {
            let handle = GetClipboardData(8)?;
            let allocation = HGLOBAL(handle.0);
            let data = GlobalLock(allocation).cast::<u8>();
            if data.is_null() {
                return Err(Error::from_thread());
            }
            let header = std::ptr::read_unaligned(data.cast::<BITMAPINFOHEADER>());
            let pixels = std::slice::from_raw_parts(
                data.add(std::mem::size_of::<BITMAPINFOHEADER>()),
                image.pixels.len(),
            );
            let matches = header.biWidth == 7
                && header.biHeight == -3
                && header.biBitCount == 32
                && pixels == image.pixels;
            let _ = GlobalUnlock(allocation);
            if !matches {
                return Err(Error::new(
                    E_FAIL,
                    "Clipboard DIB differs from exported pixels",
                ));
            }
            Ok(())
        })();
        let _ = CloseClipboard();
        result?;
        for text in ["#0C2238", "é色 🖌", ""] {
            let sequence = copy_annotation_marker(owner.0)?;
            if !annotation_clipboard_matches(sequence) {
                return Err(Error::new(E_FAIL, "Annotation clipboard marker missing"));
            }
            copy_text(owner.0, text)?;
            if annotation_clipboard_matches(sequence) {
                return Err(Error::new(
                    E_FAIL,
                    "Text did not invalidate annotation clipboard",
                ));
            }
            OpenClipboard(Some(owner.0))?;
            let result = (|| -> Result<()> {
                let handle = GetClipboardData(13)?;
                let allocation = HGLOBAL(handle.0);
                let data = GlobalLock(allocation).cast::<u16>();
                if data.is_null() {
                    return Err(Error::from_thread());
                }
                let expected: Vec<_> = text.encode_utf16().chain(Some(0)).collect();
                let matches = GlobalSize(allocation) >= expected.len() * 2
                    && std::slice::from_raw_parts(data, expected.len()) == expected;
                let _ = GlobalUnlock(allocation);
                if !matches {
                    return Err(Error::new(E_FAIL, "Clipboard Unicode text differs"));
                }
                Ok(())
            })();
            let _ = CloseClipboard();
            result?;
        }
        if copy_text(owner.0, "embedded\0null").is_ok() {
            return Err(Error::new(E_FAIL, "Embedded null accepted"));
        }
    }
    Ok(())
}
pub fn main() {
    if let Err(error) = verify() {
        eprintln!("Clipboard probe: {error}");
        std::process::exit(1);
    }
    println!("Clipboard roundtrip PASS (isolated window station)");
}
