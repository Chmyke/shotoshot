//! The Settings window driven through its own messages, hidden: a change of the pointer
//! effects settles in one state and stays there.
use super::super::display_tests::Resident;
use super::*;
use shotoshot_ui::settings_panel::{Card, Field, Item};

/// What the Halo card shows: open, switched on, the pointer effects on, the shape.
type Shown = (Option<Card>, bool, bool, usize);

struct Opened {
    resident: Resident,
    hwnd: HWND,
}
impl Opened {
    fn new() -> Self {
        let resident = Resident::new();
        let owner = resident.window.0;
        let hwnd = resident
            .state
            .borrow_mut()
            .create_settings_window(owner)
            .unwrap();
        let opened = Self { resident, hwnd };
        opened.with(|panel| panel.show_page(Page::Pointer));
        opened
    }
    fn with<T>(&self, action: impl FnOnce(&mut Panel) -> T) -> T {
        let mut app = self.resident.state.borrow_mut();
        action(&mut app.settings_ui.window.as_mut().unwrap().panel)
    }
    fn shown(&self) -> Shown {
        self.with(|panel| {
            (
                panel.open,
                panel.card_on(Card::Halo),
                panel.halo_on,
                panel.settings.halo_shape.index(),
            )
        })
    }
    /// Sends `msg` at logical point `p` of the window.
    fn mouse(&self, msg: u32, p: shotoshot_core::Point) {
        let scale = unsafe { GetDpiForWindow(self.hwnd) } as f64 / 96.;
        let (x, y) = ((p.x * scale).round() as i32, (p.y * scale).round() as i32);
        let l = ((y as u32 & 0xffff) << 16 | (x as u32 & 0xffff)) as isize;
        unsafe {
            SendMessageW(self.hwnd, msg, Some(WPARAM(0)), Some(LPARAM(l)));
        }
    }
    fn click(&self, p: shotoshot_core::Point) {
        self.mouse(WM_LBUTTONDOWN, p);
        self.mouse(WM_LBUTTONUP, p);
    }
    /// Runs the window's messages for `ms`, and returns every state the card showed.
    fn run(&self, ms: u64) -> Vec<Shown> {
        let mut seen = vec![self.shown()];
        let end = std::time::Instant::now() + std::time::Duration::from_millis(ms);
        while std::time::Instant::now() < end {
            unsafe {
                let mut msg = MSG::default();
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                    let now = self.shown();
                    if seen.last() != Some(&now) {
                        seen.push(now);
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        seen
    }
    fn control(&self, item: Item) -> shotoshot_core::Rect {
        self.with(|panel| panel.control(item).unwrap())
    }
}

/// After `seen` settled once, it stays: no state comes back.
fn assert_settles(seen: &[Shown], what: &str) {
    assert!(
        seen.len() <= 2,
        "{what}: the Halo card went through {} states: {seen:?}",
        seen.len()
    );
}

#[test]
fn switching_the_halo_card_off_settles() {
    let opened = Opened::new();
    opened.run(100);
    let switch = opened.control(Item::Card(Card::Halo));
    let at = shotoshot_core::Point {
        x: switch.max.x - 8.,
        y: switch.center().y,
    };
    opened.click(at);
    assert_settles(&opened.run(400), "switched");
    opened.click(at);
    assert_settles(&opened.run(400), "switched back");
}

#[test]
fn folding_the_halo_card_settles() {
    let opened = Opened::new();
    opened.run(100);
    let header = opened.control(Item::Card(Card::Halo));
    let at = shotoshot_core::Point {
        x: header.min.x - 40.,
        y: header.center().y,
    };
    opened.click(at);
    let seen = opened.run(400);
    assert_settles(&seen, "folded");
    assert_eq!(seen.last().unwrap().0, None, "the card folded");
}

#[test]
fn dragging_the_halo_opacity_settles() {
    let opened = Opened::new();
    opened.run(100);
    let slider = opened.control(Item::Field(Field::HaloOpacity));
    let at = |fraction: f64| shotoshot_core::Point {
        x: slider.min.x + 8. + fraction * (slider.size().x - 16.),
        y: slider.center().y,
    };
    opened.mouse(WM_LBUTTONDOWN, at(0.9));
    for step in 0..10 {
        opened.mouse(WM_MOUSEMOVE, at(0.9 - step as f64 * 0.05));
        opened.run(20);
    }
    opened.mouse(WM_LBUTTONUP, at(0.4));
    assert_settles(&opened.run(400), "dragged");
}
