//! ROLE: The documented desktop wallpaper API: today's screens with their stable ids, what each
//! shows, and one picture per screen.
//! INVARIANTS: Reading never changes the desktop; ids are single tokens usable in a text file.
use shotoshot_core::{Point, Rect, board::Screen};
use std::path::{Path, PathBuf};
use windows::{
    Win32::{
        System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree},
        UI::{
            Shell::{DESKTOP_WALLPAPER_POSITION, DesktopWallpaper, IDesktopWallpaper},
            WindowsAndMessaging::{
                SPI_SETDESKWALLPAPER, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SystemParametersInfoW,
            },
        },
    },
    core::{HSTRING, PWSTR, Result, w},
};

/// How the picture fills a screen. The order is that of `DESKTOP_WALLPAPER_POSITION`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Position {
    Center,
    Tile,
    Stretch,
    Fit,
    Fill,
    Span,
}
impl Position {
    pub const ALL: [Self; 6] = [
        Self::Center,
        Self::Tile,
        Self::Stretch,
        Self::Fit,
        Self::Fill,
        Self::Span,
    ];
    pub fn index(self) -> u32 {
        Self::ALL.iter().position(|p| *p == self).unwrap() as u32
    }
    pub fn from_index(index: u32) -> Option<Self> {
        Self::ALL.get(index as usize).copied()
    }
}

pub struct Wallpaper(IDesktopWallpaper);
impl Wallpaper {
    /// PITFALL: the calling thread must have COM initialized (`ComApartment`).
    pub fn new() -> Result<Self> {
        Ok(Self(unsafe {
            CoCreateInstance(&DesktopWallpaper, None, CLSCTX_ALL)?
        }))
    }
    /// Reads and frees a string the shell allocated for us.
    fn text(owned: PWSTR) -> String {
        if owned.is_null() {
            return String::new();
        }
        let text = unsafe { owned.to_string() }.unwrap_or_default();
        unsafe { CoTaskMemFree(Some(owned.0 as *const _)) };
        text
    }
    /// Every screen the desktop knows, with its bounds in desktop coordinates. A screen whose
    /// rectangle cannot be read (just unplugged) is left out.
    pub fn screens(&self) -> Result<Vec<Screen>> {
        let mut screens = Vec::new();
        for index in 0..unsafe { self.0.GetMonitorDevicePathCount()? } {
            let id = Self::text(unsafe { self.0.GetMonitorDevicePathAt(index)? });
            if id.is_empty() || id.contains(char::is_whitespace) {
                continue;
            }
            let Ok(r) = (unsafe { self.0.GetMonitorRECT(&HSTRING::from(id.as_str())) }) else {
                continue;
            };
            if r.right <= r.left || r.bottom <= r.top {
                continue;
            }
            let corner = |x: i32, y: i32| Point {
                x: x as f64,
                y: y as f64,
            };
            let Some(bounds) = Rect::new(corner(r.left, r.top), corner(r.right, r.bottom)) else {
                continue;
            };
            screens.push(Screen { id, bounds });
        }
        Ok(screens)
    }
    /// The picture a screen shows; `None` for a solid colour.
    pub fn shown(&self, id: &str) -> Result<Option<PathBuf>> {
        let path = Self::text(unsafe { self.0.GetWallpaper(&HSTRING::from(id))? });
        Ok((!path.is_empty()).then(|| PathBuf::from(path)))
    }
    pub fn position(&self) -> Result<Position> {
        let position = unsafe { self.0.GetPosition()? };
        Ok(Position::from_index(position.0 as u32).unwrap_or(Position::Fill))
    }
    /// The solid colour shown behind (or instead of) the pictures.
    pub fn color(&self) -> Result<[u8; 3]> {
        // COLORREF is 0x00BBGGRR.
        let c = unsafe { self.0.GetBackgroundColor()? }.0;
        Ok([
            (c & 0xff) as u8,
            ((c >> 8) & 0xff) as u8,
            ((c >> 16) & 0xff) as u8,
        ])
    }
    pub fn set(&self, id: &str, path: &Path) -> Result<()> {
        unsafe {
            self.0
                .SetWallpaper(&HSTRING::from(id), &HSTRING::from(path.as_os_str()))
        }
    }
    pub fn set_position(&self, position: Position) -> Result<()> {
        unsafe {
            self.0
                .SetPosition(DESKTOP_WALLPAPER_POSITION(position.index() as i32))
        }
    }
    /// Every screen back to the solid colour (the probe of 2026-10-10 showed the per-screen call
    /// has no documented way to do this).
    pub fn clear_all(&self) -> Result<()> {
        unsafe {
            // An empty picture name removes the wallpaper; the call only reads the string.
            SystemParametersInfoW(
                SPI_SETDESKWALLPAPER,
                0,
                Some(w!("").as_ptr() as *mut _),
                SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Reads the real screens and never writes to them; `check.ps1` runs it on its own.
    #[test]
    #[ignore = "real desktop: reads the screens and what they show, run alone by check.ps1"]
    fn real_desktop_reports_at_least_one_screen_with_a_stable_id() {
        let _com = crate::ComApartment::new().unwrap();
        let wallpaper = Wallpaper::new().unwrap();
        let screens = wallpaper.screens().unwrap();
        assert!(!screens.is_empty());
        for screen in &screens {
            assert!(screen.id.starts_with("\\\\?\\"), "{}", screen.id);
            assert!(!screen.id.contains(' '));
            assert!(screen.bounds.size().x >= 1. && screen.bounds.size().y >= 1.);
            // Reading never changes anything.
            let _ = wallpaper.shown(&screen.id).unwrap();
        }
        let _ = wallpaper.position().unwrap();
        let _ = wallpaper.color().unwrap();
        assert_eq!(
            Position::from_index(Position::Span.index()),
            Some(Position::Span)
        );
    }

    #[test]
    fn positions_keep_the_order_of_the_win32_enumeration() {
        use windows::Win32::UI::Shell::{
            DWPOS_CENTER, DWPOS_FILL, DWPOS_FIT, DWPOS_SPAN, DWPOS_STRETCH, DWPOS_TILE,
        };
        let win32 = [
            DWPOS_CENTER,
            DWPOS_TILE,
            DWPOS_STRETCH,
            DWPOS_FIT,
            DWPOS_FILL,
            DWPOS_SPAN,
        ];
        for (position, expected) in Position::ALL.into_iter().zip(win32) {
            assert_eq!(position.index() as i32, expected.0, "{position:?}");
        }
        assert_eq!(Position::from_index(6), None);
    }
}
