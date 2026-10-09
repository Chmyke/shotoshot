//! Monitor geometry for the portable capture: the pointer's monitor for the hint, and
//! every monitor with its display scale for the full-screen button and the interface size.
use super::*;
use shotoshot_ui::Monitor;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};

impl Session {
    /// Every monitor's bounds, in frame coordinates, with its interface scale.
    pub fn frame_monitors(&self) -> Vec<Monitor> {
        unsafe extern "system" fn collect(
            monitor: HMONITOR,
            _: HDC,
            bounds: *mut RECT,
            list: LPARAM,
        ) -> BOOL {
            // A failed query keeps 96 DPI: the interface at its 100 % size.
            let (mut dpi, mut vertical) = (96, 96);
            unsafe {
                let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi, &mut vertical);
                (*(list.0 as *mut Vec<(RECT, u32)>)).push((*bounds, dpi));
            }
            TRUE
        }
        let mut list = Vec::<(RECT, u32)>::new();
        unsafe {
            let _ = EnumDisplayMonitors(
                None,
                None,
                Some(collect),
                LPARAM(&mut list as *mut Vec<(RECT, u32)> as isize),
            );
        }
        let image = &self.capture.image;
        list.iter()
            .map(|(r, dpi)| Monitor {
                bounds: Rect {
                    min: Point {
                        x: (r.left - image.left) as f64,
                        y: (r.top - image.top) as f64,
                    },
                    max: Point {
                        x: (r.right - image.left) as f64,
                        y: (r.bottom - image.top) as f64,
                    },
                },
                scale: *dpi as f64 / 96.,
            })
            .collect()
    }
    /// Monitor under the pointer, in frame coordinates; the whole frame if unknown.
    pub(super) fn pointer_monitor(&self) -> Rect {
        let image = &self.capture.image;
        let frame = Rect {
            min: Point::default(),
            max: Point {
                x: image.width as f64,
                y: image.height as f64,
            },
        };
        unsafe {
            let monitor = MonitorFromPoint(
                POINT {
                    x: self.capture.pointer.x as i32 + image.left,
                    y: self.capture.pointer.y as i32 + image.top,
                },
                MONITOR_DEFAULTTONEAREST,
            );
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if !GetMonitorInfoW(monitor, &mut info).as_bool() {
                return frame;
            }
            Rect {
                min: Point {
                    x: (info.rcMonitor.left - image.left) as f64,
                    y: (info.rcMonitor.top - image.top) as f64,
                },
                max: Point {
                    x: (info.rcMonitor.right - image.left) as f64,
                    y: (info.rcMonitor.bottom - image.top) as f64,
                },
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hint_stays_on_current_monitor_and_away_from_pointer() {
        let screen = Rect {
            min: Point { x: 1920., y: 100. },
            max: Point { x: 3840., y: 1180. },
        };
        for pointer in [screen.min, screen.max, Point { x: 2000., y: 1100. }] {
            let bounds = shotoshot_ui::place_hint(pointer, screen, Point { x: 170., y: 48. }, 1.);
            assert!(screen.contains(bounds.min) && screen.contains(bounds.max));
            assert!(!bounds.contains(pointer));
        }
    }
    #[test]
    fn hint_disappears_on_press_and_returns_after_empty_selection() {
        let _com = crate::ComApartment::new().unwrap();
        let mut session = Session::new(Image::solid(400, 300, [0, 0, 0, 255])).unwrap();
        assert!(session.capture.awaiting_region());
        session.capture.region_origin = Some(Point { x: 50., y: 50. });
        assert!(!session.capture.awaiting_region());
        session.mouse_up(HWND::default(), Point { x: 50., y: 50. });
        assert!(session.capture.awaiting_region());
        session.capture.region_origin = Some(Point { x: 50., y: 50. });
        session.mouse_up(HWND::default(), Point { x: 150., y: 150. });
        assert!(!session.capture.awaiting_region());
    }
}
