//! Direct manipulation of the input diagram without activating its windows.
use super::*;

impl Presentation {
    pub(super) fn show_lock(&mut self) -> Result<()> {
        let Some(keys) = &self.keys else {
            return Ok(());
        };
        let mut bounds = RECT::default();
        unsafe {
            GetWindowRect(keys.hwnd(), &mut bounds)?;
        }
        let plan = keys.plan();
        let scale = keys.width as f64 / plan.width() as f64;
        let size = (diagram::LOCK_SIZE as f64 * scale) as i32;
        let (x, y) = plan.lock();
        if self.lock.as_ref().is_none_or(|layer| layer.width != size) {
            self.lock = Some(Layer::new(self.address, 0, 0, size, size)?);
        }
        if let Some(lock) = &self.lock {
            lock.mouse_interactive(true)?;
            lock.position(
                bounds.left + (x as f64 * scale) as i32,
                bounds.top + (y as f64 * scale) as i32,
            )?;
            self.paint(lock.hwnd())?;
            lock.show(true);
        }
        Ok(())
    }
    pub(super) fn move_input_drag(&mut self) -> Result<()> {
        let Some((start, origin)) = self.input_drag else {
            return Ok(());
        };
        let point = cursor()?;
        let Some(keys) = &self.keys else {
            return Ok(());
        };
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        unsafe {
            GetMonitorInfoW(MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST), &mut info).ok()?;
        }
        let room_x = (info.rcWork.right - info.rcWork.left - keys.width).max(0);
        let room_y = (info.rcWork.bottom - info.rcWork.top - keys.height).max(0);
        let x = (origin.x + point.x - start.x - info.rcWork.left).clamp(0, room_x);
        let y = (origin.y + point.y - start.y - info.rcWork.top).clamp(0, room_y);
        self.preferences.input_offset = Some([normalize(x, room_x), normalize(y, room_y)]);
        self.input_monitor = point;
        keys.position(info.rcWork.left + x, info.rcWork.top + y)?;
        self.show_lock()
    }
    pub(super) fn end_input_drag(&mut self, save: bool) -> Result<()> {
        if self.input_drag.take().is_some() {
            unsafe {
                if self
                    .keys
                    .as_ref()
                    .is_some_and(|keys| GetCapture() == keys.hwnd())
                {
                    ReleaseCapture()?;
                }
            }
            if save {
                self.show_devices()?;
                self.input_preferences_changed()?;
            }
        }
        Ok(())
    }
    pub(super) fn input_preferences_changed(&self) -> Result<()> {
        unsafe { PostMessageW(Some(self.owner), PREFERENCES_CHANGED, WPARAM(0), LPARAM(0)) }
    }
    pub(super) fn input_window_message(&mut self, hwnd: HWND, msg: u32) -> Result<Option<LRESULT>> {
        let lock = self.lock.as_ref().is_some_and(|layer| layer.hwnd() == hwnd);
        let keys = self.keys.as_ref().is_some_and(|layer| layer.hwnd() == hwnd);
        if !lock && !keys {
            return Ok(None);
        }
        match msg {
            WM_MOUSEACTIVATE => return Ok(Some(LRESULT(MA_NOACTIVATE as isize))),
            WM_LBUTTONDOWN if lock => {
                self.end_input_drag(true)?;
                self.preferences.input_locked = !self.preferences.input_locked;
                self.show_devices()?;
                self.input_preferences_changed()?;
            }
            WM_LBUTTONDOWN if !self.preferences.input_locked => {
                let mut bounds = RECT::default();
                unsafe {
                    GetWindowRect(hwnd, &mut bounds)?;
                    SetCapture(hwnd);
                }
                self.input_drag = Some((
                    cursor()?,
                    POINT {
                        x: bounds.left,
                        y: bounds.top,
                    },
                ));
            }
            WM_MOUSEMOVE if self.input_drag.is_some() => self.move_input_drag()?,
            WM_LBUTTONUP => {
                self.move_input_drag()?;
                self.end_input_drag(true)?;
            }
            WM_CANCELMODE | WM_CAPTURECHANGED => self.end_input_drag(true)?,
            WM_SETCURSOR => {
                unsafe {
                    SetCursor(Some(LoadCursorW(
                        None,
                        if lock { IDC_HAND } else { IDC_SIZEALL },
                    )?));
                }
                return Ok(Some(LRESULT(1)));
            }
            _ => return Ok(None),
        }
        Ok(Some(LRESULT(0)))
    }
}
fn normalize(value: i32, room: i32) -> u16 {
    if room <= 0 {
        0
    } else {
        (value.clamp(0, room) as i64 * 10000 / room as i64) as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drag_offsets_stay_normalized_even_on_small_monitors() {
        assert_eq!(normalize(125, 500), 2500);
        assert_eq!(normalize(-50, 500), 0);
        assert_eq!(normalize(800, 500), 10000);
        assert_eq!(normalize(10, 0), 0);
    }
    #[test]
    fn diagram_lock_changes_hit_testing_without_allowing_activation() {
        let _com = crate::ComApartment::new().unwrap();
        let layer = Layer::new(
            std::ptr::null(),
            -32000,
            -32000,
            diagram::WIDTH,
            diagram::HEIGHT,
        )
        .unwrap();
        for active in [true, false, true] {
            layer.mouse_interactive(active).unwrap();
            assert_eq!(layer.intercepts(), active);
            let style = unsafe { GetWindowLongPtrW(layer.hwnd(), GWL_EXSTYLE) as u32 };
            assert_ne!(style & WS_EX_NOACTIVATE.0, 0);
        }
        layer.begin(true).unwrap();
        layer.devices(&Devices::default(), 0);
        layer.end().unwrap();
        assert!(layer.pixel(2, 50)[3] < 90, "the panel remains transparent");
        assert!(
            layer.pixel(12, 15)[3] > 160,
            "key caps remain more legible than the panel"
        );
        assert!(
            layer.pixel(435, 5)[3] > 0,
            "empty diagram space is also draggable"
        );
    }
}
