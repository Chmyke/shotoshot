//! ROLE: The notification menu's own windows: settings change without closing the menu.
//! OWNER: App holds the open Popup; its windows post every choice to App and never borrow it.
//! INVARIANTS: Closes on Escape, on a click outside it, or on a press on the notification icon;
//! a command (anything that opens or runs something) closes it before it runs.
use super::*;
use windows::Win32::{Graphics::Dwm::*, UI::Controls::WM_MOUSELEAVE};

/// Posted to the owner: w = row id, l = 1 when the menu stays open after it.
pub(in crate::app) const COMMAND: u32 = WM_APP + 13;
/// Posted to the owner once the menu is hidden: w = 1 when a press on the notification icon
/// closed it, so the release that follows does not open it again; l = its root window.
pub(in crate::app) const CLOSED: u32 = WM_APP + 14;
const CLASS: PCWSTR = w!("Shotoshot.Menu");
/// Shows the submenu of the row the pointer rests on, after the system's menu delay.
const SHOW_TIMER: usize = 1;
/// Empty band above the first row and below the last, in logical pixels.
const PADDING: f64 = 4.;

/// What activating a row does.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Effect {
    /// A setting: applied while the menu stays open.
    Apply(u32),
    /// Opens or runs something: the menu closes first.
    Run(u32),
    /// Shows the row's submenu.
    Open,
    None,
}
fn effect(row: &Row) -> Effect {
    match row.kind {
        Kind::Toggle | Kind::Choice => Effect::Apply(row.id),
        Kind::Command => Effect::Run(row.id),
        Kind::Submenu => Effect::Open,
        Kind::Title | Kind::Separator => Effect::None,
    }
}
fn selectable(row: &Row) -> bool {
    !matches!(row.kind, Kind::Title | Kind::Separator)
}
/// The next selectable row after `from`, wrapping around like a native menu.
fn step(rows: &[Row], from: Option<usize>, forward: bool) -> Option<usize> {
    let count = rows.len();
    let start = from.unwrap_or(if forward { count.checked_sub(1)? } else { 0 });
    (1..=count)
        .map(|offset| {
            if forward {
                (start + offset) % count
            } else {
                (start + count - offset % count) % count
            }
        })
        .find(|&index| selectable(&rows[index]))
}
/// Whether the menu lost the foreground to a press on the notification icon.
fn pressed_on_icon(icon: RECT, pointer: POINT, pressed: bool) -> bool {
    pressed
        && (icon.left..icon.right).contains(&pointer.x)
        && (icon.top..icon.bottom).contains(&pointer.y)
}

/// One column of rows, laid out in device pixels.
pub(super) struct Column {
    rows: &'static [Row],
    /// Top of each row, then the bottom of the last one.
    tops: Vec<i32>,
    pub(super) width: i32,
}
impl Column {
    pub(super) fn new(rows: &'static [Row], values: &Values, scale: f64) -> Result<Self> {
        let widest = with_painter(|renderer| {
            rows.iter().try_fold(0f64, |widest, row| {
                Ok::<_, Error>(widest.max(width(renderer, row, values.data(row.id))?))
            })
        })?;
        let mut tops = vec![(PADDING * scale).round() as i32];
        for row in rows {
            let top = tops[tops.len() - 1];
            tops.push(top + (height(row.kind) * scale).round() as i32);
        }
        Ok(Self {
            rows,
            tops,
            width: (widest * scale).ceil() as i32,
        })
    }
    pub(super) fn height(&self) -> i32 {
        self.tops[self.rows.len()] + self.tops[0]
    }
    pub(super) fn top(&self, index: usize) -> i32 {
        self.tops[index]
    }
    /// The selectable row under `y`, in the column's own pixels.
    fn at(&self, y: i32) -> Option<usize> {
        (0..self.rows.len())
            .find(|&index| (self.tops[index]..self.tops[index + 1]).contains(&y))
            .filter(|&index| selectable(&self.rows[index]))
    }
    /// Paints the column with its top-left corner at `origin` of `dc`.
    pub(super) fn paint(
        &self,
        dc: HDC,
        origin: POINT,
        values: &Values,
        selected: Option<usize>,
        scale: f64,
    ) -> Result<()> {
        let band = |top: i32, bottom: i32| RECT {
            left: origin.x,
            top: origin.y + top,
            right: origin.x + self.width,
            bottom: origin.y + bottom,
        };
        unsafe {
            let brush = CreateSolidBrush(notice::color(0x2b2b2b));
            FillRect(dc, &band(0, self.tops[0]), brush);
            FillRect(dc, &band(self.tops[self.rows.len()], self.height()), brush);
            let _ = DeleteObject(brush.into());
        }
        with_painter(|renderer| {
            for (index, row) in self.rows.iter().enumerate() {
                draw_row(
                    renderer,
                    dc,
                    band(self.tops[index], self.tops[index + 1]),
                    row,
                    State {
                        selected: selected == Some(index),
                        checked: values.checked(row.id),
                        data: values.data(row.id),
                    },
                    scale,
                )?;
            }
            Ok(())
        })
    }
}

/// Where a submenu of `size` opens beside `root`, its top wished at `top`: on the right,
/// flipped to the left when `work` ends first, and always inside `work`.
fn submenu_origin(root: RECT, top: i32, size: SIZE, work: RECT) -> POINT {
    let x = if root.right + size.cx <= work.right {
        root.right
    } else if root.left - size.cx >= work.left {
        root.left - size.cx
    } else {
        (work.right - size.cx).max(work.left)
    };
    let y = top.min(work.bottom - size.cy).max(work.top);
    POINT { x, y }
}

struct Model {
    owner: HWND,
    root: HWND,
    sub: HWND,
    /// The notification icon, in screen pixels; empty when unknown.
    icon: RECT,
    values: Values,
    scale: f64,
    /// The root rows, then the rows of the open submenu.
    columns: [Column; 2],
    /// Root row whose submenu shows.
    open: Option<usize>,
    selected: [Option<usize>; 2],
    /// The arrow keys move in the submenu.
    in_sub: bool,
    /// False for tests: windows stay off screen, where nothing fits a work area.
    on_screen: bool,
}
impl Model {
    fn window(&self, column: usize) -> HWND {
        [self.root, self.sub][column]
    }
    fn redraw(&self, column: usize) {
        unsafe {
            let _ = InvalidateRect(Some(self.window(column)), None, false);
        }
    }
    fn post(&self, msg: u32, w: usize, l: isize) {
        unsafe {
            let _ = PostMessageW(Some(self.owner), msg, WPARAM(w), LPARAM(l));
        }
    }
    fn hide(&self) {
        unsafe {
            let _ = KillTimer(Some(self.root), SHOW_TIMER);
            let _ = ShowWindow(self.sub, SW_HIDE);
            let _ = ShowWindow(self.root, SW_HIDE);
        }
    }
    fn close(&self, by_icon: bool) {
        self.hide();
        self.post(CLOSED, usize::from(by_icon), self.root.0 as isize);
    }
    /// Sizes the root window and places the open submenu beside it.
    fn place(&self) -> Result<()> {
        unsafe {
            let mut root = RECT::default();
            GetWindowRect(self.root, &mut root)?;
            SetWindowPos(
                self.root,
                Some(HWND_TOPMOST),
                0,
                0,
                self.columns[0].width,
                self.columns[0].height(),
                SWP_NOMOVE | SWP_NOACTIVATE,
            )?;
            root.right = root.left + self.columns[0].width;
            root.bottom = root.top + self.columns[0].height();
            let Some(open) = self.open else {
                return Ok(());
            };
            let sub = &self.columns[1];
            let beside = POINT {
                x: root.right,
                y: root.top + self.columns[0].top(open) - sub.top(0),
            };
            let mut placed = beside;
            if self.on_screen {
                // The root's own screen: anchored at the root's right edge, the system would
                // take the work area of the screen beyond it and open the submenu there.
                let mut monitor = MONITORINFO {
                    cbSize: size_of::<MONITORINFO>() as u32,
                    ..Default::default()
                };
                GetMonitorInfoW(
                    MonitorFromRect(&root, MONITOR_DEFAULTTONEAREST),
                    &mut monitor,
                )
                .ok()?;
                placed = submenu_origin(
                    root,
                    beside.y,
                    SIZE {
                        cx: sub.width,
                        cy: sub.height(),
                    },
                    monitor.rcWork,
                );
            }
            SetWindowPos(
                self.sub,
                Some(HWND_TOPMOST),
                placed.x,
                placed.y,
                sub.width,
                sub.height(),
                SWP_NOACTIVATE,
            )
        }
    }
    fn set_open(&mut self, open: Option<usize>) -> Result<()> {
        if open == self.open {
            return Ok(());
        }
        self.open = open;
        self.selected[1] = None;
        self.in_sub = false;
        let rows = open.map_or(&[][..], |index| choices(ROOT[index].id));
        self.columns[1] = Column::new(rows, &self.values, self.scale)?;
        if open.is_some() {
            self.place()?;
            unsafe {
                let _ = ShowWindow(self.sub, SW_SHOWNOACTIVATE);
            }
            self.redraw(1);
        } else {
            unsafe {
                let _ = ShowWindow(self.sub, SW_HIDE);
            }
        }
        Ok(())
    }
    fn activate(&mut self, column: usize, index: usize, keyboard: bool) -> Result<()> {
        match effect(&self.columns[column].rows[index]) {
            Effect::Apply(id) => self.post(COMMAND, id as usize, 1),
            Effect::Run(id) => {
                self.hide();
                self.post(COMMAND, id as usize, 0);
            }
            Effect::Open => {
                self.set_open(Some(index))?;
                if keyboard {
                    self.in_sub = true;
                    self.selected[1] = step(self.columns[1].rows, None, true);
                    self.redraw(1);
                }
            }
            Effect::None => {}
        }
        Ok(())
    }
    fn hover(&mut self, column: usize, index: Option<usize>) {
        if column == 1 {
            // Reaching the submenu keeps it, whatever rows the pointer crossed on its way.
            unsafe {
                let _ = KillTimer(Some(self.root), SHOW_TIMER);
            }
            if self.selected[0] != self.open {
                self.selected[0] = self.open;
                self.redraw(0);
            }
            self.in_sub = true;
            if index.is_some() && index != self.selected[1] {
                self.selected[1] = index;
                self.redraw(1);
            }
            return;
        }
        if index.is_none() || index == self.selected[0] {
            return;
        }
        self.selected[0] = index;
        self.in_sub = false;
        self.redraw(0);
        let mut delay = 400u32;
        unsafe {
            let _ = SystemParametersInfoW(
                SPI_GETMENUSHOWDELAY,
                0,
                Some((&mut delay as *mut u32).cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            );
            SetTimer(Some(self.root), SHOW_TIMER, delay.max(1), None);
        }
    }
    /// The submenu follows the row the pointer rested on.
    fn rested(&mut self) -> Result<()> {
        unsafe {
            let _ = KillTimer(Some(self.root), SHOW_TIMER);
        }
        let open = self.selected[0].filter(|&index| ROOT[index].kind == Kind::Submenu);
        self.set_open(open)
    }
    fn left(&mut self, column: usize) {
        // The row whose submenu shows stays lit, as in a native menu.
        if column == 1 || self.open.is_none() {
            self.selected[column] = None;
            self.redraw(column);
        }
    }
    fn key(&mut self, key: VIRTUAL_KEY) -> Result<()> {
        let column = usize::from(self.in_sub);
        match key {
            VK_DOWN | VK_UP => {
                let forward = key == VK_DOWN;
                self.selected[column] =
                    step(self.columns[column].rows, self.selected[column], forward);
                if column == 0 {
                    self.set_open(None)?;
                }
                self.redraw(column);
            }
            VK_RIGHT if column == 0 => {
                if let Some(index) = self.selected[0]
                    && ROOT[index].kind == Kind::Submenu
                {
                    self.activate(0, index, true)?;
                }
            }
            VK_LEFT if column == 1 => self.set_open(None)?,
            VK_ESCAPE if self.open.is_some() => self.set_open(None)?,
            VK_ESCAPE => self.close(false),
            VK_RETURN | VK_SPACE => {
                if let Some(index) = self.selected[column] {
                    self.activate(column, index, true)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn paint(&self, hwnd: HWND, column: usize) -> Result<()> {
        unsafe {
            let mut paint = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut paint);
            let painted = self.columns[column].paint(
                dc,
                POINT::default(),
                &self.values,
                self.selected[column],
                self.scale,
            );
            let _ = EndPaint(hwnd, &paint);
            painted
        }
    }
    fn handle(&mut self, hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> Result<Option<LRESULT>> {
        let column = usize::from(hwnd == self.sub);
        let y = (l.0 >> 16) as u16 as i16 as i32;
        match msg {
            WM_PAINT => self.paint(hwnd, column)?,
            WM_MOUSEMOVE => {
                unsafe {
                    let _ = TrackMouseEvent(&mut TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        ..Default::default()
                    });
                }
                self.hover(column, self.columns[column].at(y));
            }
            WM_MOUSELEAVE => self.left(column),
            // Released, as in a native menu, with either button.
            WM_LBUTTONUP | WM_RBUTTONUP => {
                if let Some(index) = self.columns[column].at(y) {
                    self.activate(column, index, false)?;
                }
            }
            WM_KEYDOWN => self.key(VIRTUAL_KEY(w.0 as u16))?,
            WM_TIMER if w.0 == SHOW_TIMER => self.rested()?,
            WM_ACTIVATE if w.0 & 0xffff == WA_INACTIVE as usize => {
                let mut pointer = POINT::default();
                let pressed = unsafe {
                    let _ = GetCursorPos(&mut pointer);
                    GetAsyncKeyState(VK_LBUTTON.0 as i32) < 0
                        || GetAsyncKeyState(VK_RBUTTON.0 as i32) < 0
                };
                self.close(pressed_on_icon(self.icon, pointer, pressed));
            }
            _ => return Ok(None),
        }
        Ok(Some(LRESULT(0)))
    }
}

unsafe extern "system" fn popup_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    // PITFALL: Win32 calls made while the model is borrowed send messages back here;
    // those find it borrowed and get the default handling.
    let result = std::panic::catch_unwind(|| unsafe {
        match msg {
            // The root keeps the foreground; clicking the submenu must not take it.
            WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
            WM_ERASEBKGND => return LRESULT(1),
            _ => {}
        }
        let model = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<Model>;
        if !model.is_null()
            && let Ok(mut model) = (*model).try_borrow_mut()
        {
            // Best effort: a failed paint or submenu leaves the menu usable.
            if let Ok(Some(result)) = model.handle(hwnd, msg, w, l) {
                return result;
            }
        }
        DefWindowProcW(hwnd, msg, w, l)
    });
    result.unwrap_or(LRESULT(0))
}

/// The open menu. Dropping it destroys its windows and releases the painter.
pub(in crate::app) struct Popup {
    model: Box<RefCell<Model>>,
    _painter: Open,
}
impl Popup {
    /// Opens the menu at `at`, in front and with the keyboard. `icon` is the notification
    /// icon's screen rectangle. Off screen is for tests: nothing shows or takes the foreground.
    pub(in crate::app) fn open(
        owner: HWND,
        icon: RECT,
        values: Values,
        at: POINT,
        on_screen: bool,
    ) -> Result<Self> {
        let painter = Open;
        unsafe {
            let instance = GetModuleHandleW(None)?;
            if RegisterClassW(&WNDCLASSW {
                style: CS_DROPSHADOW,
                lpfnWndProc: Some(popup_proc),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                lpszClassName: CLASS,
                ..Default::default()
            }) == 0
                && GetLastError() != ERROR_CLASS_ALREADY_EXISTS
            {
                return Err(Error::from_thread());
            }
            let create = |style: WINDOW_EX_STYLE, parent: Option<HWND>| {
                CreateWindowExW(
                    WS_EX_TOOLWINDOW | WS_EX_TOPMOST | style,
                    CLASS,
                    w!("Shotoshot menu"),
                    WS_POPUP,
                    at.x,
                    at.y,
                    1,
                    1,
                    parent,
                    None,
                    Some(instance.into()),
                    None,
                )
            };
            let root = crate::resources::OwnedWindow(create(WINDOW_EX_STYLE(0), None)?);
            // Owned by the root: it stays above it and dies with it.
            let sub = create(WS_EX_NOACTIVATE, Some(root.0))?;
            let scale = GetDpiForWindow(root.0).max(96) as f64 / 96.;
            for window in [root.0, sub] {
                let corners = DWMWCP_ROUNDSMALL;
                let _ = DwmSetWindowAttribute(
                    window,
                    DWMWA_WINDOW_CORNER_PREFERENCE,
                    (&corners as *const DWM_WINDOW_CORNER_PREFERENCE).cast(),
                    std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
                );
                let border = notice::color(0x494949);
                let _ = DwmSetWindowAttribute(
                    window,
                    DWMWA_BORDER_COLOR,
                    (&border as *const COLORREF).cast(),
                    std::mem::size_of::<COLORREF>() as u32,
                );
            }
            let model = Box::new(RefCell::new(Model {
                owner,
                root: root.0,
                sub,
                icon,
                columns: [
                    Column::new(ROOT, &values, scale)?,
                    Column::new(&[], &values, scale)?,
                ],
                values,
                scale,
                open: None,
                selected: [None; 2],
                in_sub: false,
                on_screen,
            }));
            for window in [root.0, sub] {
                SetWindowLongPtrW(window, GWLP_USERDATA, &*model as *const _ as isize);
            }
            let popup = Self {
                model,
                _painter: painter,
            };
            // The popup now destroys both windows.
            std::mem::forget(root);
            let size = {
                let model = popup.model.borrow();
                SIZE {
                    cx: model.columns[0].width,
                    cy: model.columns[0].height(),
                }
            };
            let mut placed = RECT {
                left: at.x,
                top: at.y,
                ..Default::default()
            };
            if on_screen {
                // Beside the pointer and inside the work area, flipped like a native menu.
                CalculatePopupWindowPosition(
                    &at,
                    &size,
                    (TPM_LEFTALIGN | TPM_TOPALIGN | TPM_WORKAREA).0,
                    None,
                    &mut placed,
                )?;
            }
            SetWindowPos(
                popup.root(),
                Some(HWND_TOPMOST),
                placed.left,
                placed.top,
                size.cx,
                size.cy,
                SWP_NOACTIVATE,
            )?;
            if on_screen {
                let _ = ShowWindow(popup.root(), SW_SHOW);
                let _ = SetForegroundWindow(popup.root());
            }
            Ok(popup)
        }
    }
    pub(in crate::app) fn root(&self) -> HWND {
        self.model.borrow().root
    }
    /// Shows `values`, after App applied a setting.
    pub(in crate::app) fn update(&self, values: Values) -> Result<()> {
        let mut model = self.model.borrow_mut();
        let rows = model.open.map_or(&[][..], |index| choices(ROOT[index].id));
        model.columns = [
            Column::new(ROOT, &values, model.scale)?,
            Column::new(rows, &values, model.scale)?,
        ];
        model.values = values;
        model.place()?;
        model.redraw(0);
        model.redraw(1);
        Ok(())
    }
}
impl Drop for Popup {
    fn drop(&mut self) {
        let model = self.model.borrow();
        unsafe {
            // ORDER: The windows forget the model before it goes.
            for window in [model.sub, model.root] {
                SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            }
            // Destroys the owned submenu too.
            let _ = DestroyWindow(model.root);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::*;

    fn index(id: u32) -> usize {
        ROOT.iter().position(|row| row.id == id).unwrap()
    }
    fn owner() -> crate::resources::OwnedWindow {
        crate::resources::OwnedWindow(unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                w!("STATIC"),
                w!("Menu owner test"),
                WS_POPUP,
                -32000,
                -32000,
                1,
                1,
                None,
                None,
                None,
                None,
            )
            .unwrap()
        })
    }
    fn hidden(owner: HWND) -> Popup {
        Popup::open(
            owner,
            RECT::default(),
            Values::default(),
            POINT {
                x: -32000,
                y: -32000,
            },
            false,
        )
        .unwrap()
    }
    /// The messages the menu posted to `owner`, oldest first.
    fn posted(owner: HWND) -> Vec<(u32, usize, isize)> {
        let mut found = Vec::new();
        let mut message = MSG::default();
        unsafe {
            while PeekMessageW(&mut message, Some(owner), COMMAND, CLOSED, PM_REMOVE).as_bool() {
                found.push((message.message, message.wParam.0, message.lParam.0));
            }
        }
        found
    }
    unsafe fn click(window: HWND, column: &Column, row: usize) {
        let y = (column.top(row) + column.top(row + 1)) / 2;
        let at = LPARAM(((y as isize) << 16) | 20);
        unsafe {
            SendMessageW(window, WM_MOUSEMOVE, None, Some(at));
            SendMessageW(window, WM_LBUTTONUP, None, Some(at));
        }
    }

    #[test]
    fn settings_stay_open_and_commands_close() {
        for row in ROOT {
            let expected = match row.id {
                // Switches: the menu stays open to set several in a row.
                101..=104 | 107 | 9 => Effect::Apply(row.id),
                // It opens a submenu, which stays open on its choices too.
                150 => Effect::Open,
                // Each opens or runs something else; a shape row records its shortcut.
                1 | 3 | 7 | 8 | 10 | 170..=173 => Effect::Run(row.id),
                _ => Effect::None,
            };
            assert_eq!(effect(row), expected, "{}", label_in(row, Language::En));
        }
        // A language is a setting too: the menu shows it at once.
        for choice in &LANGUAGES {
            assert_eq!(
                effect(choice),
                Effect::Apply(choice.id),
                "{}",
                label_in(choice, Language::En)
            );
        }
    }

    #[test]
    fn arrow_keys_skip_titles_and_wrap() {
        assert_eq!(step(ROOT, None, true), Some(0));
        assert_eq!(step(ROOT, None, false), Some(ROOT.len() - 1));
        // From the last board row down to the first switch, over the title.
        assert_eq!(step(ROOT, Some(index(10)), true), Some(index(101)));
        assert_eq!(step(ROOT, Some(index(101)), false), Some(index(10)));
        // Through the shape shortcuts, then to the settings over the separator, and around.
        assert_eq!(step(ROOT, Some(index(102)), true), Some(index(171)));
        assert_eq!(step(ROOT, Some(index(173)), true), Some(index(170)));
        assert_eq!(step(ROOT, Some(index(150)), true), Some(index(3)));
        assert_eq!(step(ROOT, Some(index(3)), true), Some(0));
        assert_eq!(step(&[], None, true), None);
        let all_inert = &ROOT[index(90)..=index(90)];
        assert_eq!(step(all_inert, None, true), None);
    }

    #[test]
    fn only_a_press_on_the_icon_counts_as_closing_it() {
        let icon = RECT {
            left: 100,
            top: 200,
            right: 124,
            bottom: 224,
        };
        let inside = POINT { x: 110, y: 210 };
        assert!(pressed_on_icon(icon, inside, true));
        assert!(
            !pressed_on_icon(icon, inside, false),
            "the keyboard or a release"
        );
        assert!(!pressed_on_icon(icon, POINT { x: 124, y: 210 }, true));
        assert!(!pressed_on_icon(RECT::default(), POINT::default(), true));
    }

    #[test]
    fn rows_are_found_under_the_pointer_but_titles_never() {
        let _open = Open;
        let column = Column::new(ROOT, &Values::default(), 2.).unwrap();
        assert_eq!(column.top(0), 8, "padding above the first row");
        assert_eq!(
            column.height(),
            8 + ROOT
                .iter()
                .map(|row| (height(row.kind) * 2.).round() as i32)
                .sum::<i32>()
                + 8
        );
        assert_eq!(column.at(0), None, "padding");
        assert_eq!(column.at(column.top(0)), Some(0));
        assert_eq!(column.at(column.top(1) - 1), Some(0));
        assert_eq!(column.at(column.top(index(90)) + 4), None, "title");
        assert_eq!(column.at(column.top(index(3))), Some(index(3)));
        assert_eq!(column.at(column.height() - 1), None, "padding");
    }

    #[test]
    fn a_setting_keeps_the_menu_open_and_a_command_closes_it() {
        let owner = owner();
        let popup = hidden(owner.0);
        unsafe {
            let _ = ShowWindow(popup.root(), SW_SHOWNOACTIVATE);
            let root = popup.model.borrow().columns[0].tops.clone();
            let column = Column {
                rows: ROOT,
                tops: root,
                width: 0,
            };
            click(popup.root(), &column, index(101));
            assert_eq!(posted(owner.0), [(COMMAND, 101, 1)]);
            assert!(IsWindowVisible(popup.root()).as_bool());
            click(popup.root(), &column, index(90));
            assert_eq!(posted(owner.0), [], "a title does nothing");
            click(popup.root(), &column, index(170));
            assert_eq!(posted(owner.0), [(COMMAND, 170, 0)]);
            assert!(!IsWindowVisible(popup.root()).as_bool());
        }
    }

    #[test]
    fn a_submenu_stays_on_the_screen_of_its_menu() {
        let rect = |left, top, right, bottom| RECT {
            left,
            top,
            right,
            bottom,
        };
        let size = SIZE { cx: 200, cy: 300 };
        // The left screen of two, the menu against its right edge: the submenu flips left
        // instead of opening on the right screen.
        let work = rect(0, 0, 1920, 1040);
        let root = rect(1620, 500, 1920, 900);
        assert_eq!(
            submenu_origin(root, 600, size, work),
            POINT { x: 1420, y: 600 }
        );
        // Room on the right: beside the menu, pulled up above the taskbar.
        let root = rect(100, 700, 400, 1000);
        assert_eq!(
            submenu_origin(root, 900, size, work),
            POINT { x: 400, y: 740 }
        );
        // The same on the right screen, whose coordinates start at 1920.
        let work = rect(1920, 0, 3840, 1040);
        let root = rect(3540, 100, 3840, 400);
        assert_eq!(
            submenu_origin(root, 150, size, work),
            POINT { x: 3340, y: 150 }
        );
    }

    #[test]
    fn a_submenu_choice_keeps_both_open() {
        let owner = owner();
        let popup = hidden(owner.0);
        unsafe {
            let _ = ShowWindow(popup.root(), SW_SHOWNOACTIVATE);
            let tops = popup.model.borrow().columns[0].tops.clone();
            click(
                popup.root(),
                &Column {
                    rows: ROOT,
                    tops,
                    width: 0,
                },
                index(150),
            );
            let (sub, tops) = {
                let model = popup.model.borrow();
                assert_eq!(model.open, Some(index(150)));
                (model.sub, model.columns[1].tops.clone())
            };
            assert!(IsWindowVisible(sub).as_bool());
            click(
                sub,
                &Column {
                    rows: &LANGUAGES,
                    tops,
                    width: 0,
                },
                6,
            );
            assert_eq!(posted(owner.0), [(COMMAND, 157, 1)]);
            assert!(IsWindowVisible(sub).as_bool() && IsWindowVisible(popup.root()).as_bool());
            // The shown choice follows the setting App applied.
            popup
                .update(Values {
                    language: LanguageChoice::Fixed(Language::De),
                    ..Default::default()
                })
                .unwrap();
            assert!(popup.model.borrow().values.checked(157));
        }
    }

    #[test]
    fn the_keyboard_walks_the_menu_and_escape_closes_it() {
        let owner = owner();
        let popup = hidden(owner.0);
        let key = |vk: VIRTUAL_KEY| unsafe {
            SendMessageW(popup.root(), WM_KEYDOWN, Some(WPARAM(vk.0 as usize)), None);
        };
        // Down to the first switch, which Space turns on without closing.
        for _ in 0..6 {
            key(VK_DOWN);
        }
        assert_eq!(popup.model.borrow().selected[0], Some(index(101)));
        key(VK_SPACE);
        assert_eq!(posted(owner.0), [(COMMAND, 101, 1)]);
        // Past the other switches, the shape shortcuts and the settings, into the languages.
        for _ in 0..9 {
            key(VK_DOWN);
        }
        assert_eq!(popup.model.borrow().selected[0], Some(index(150)));
        key(VK_RIGHT);
        key(VK_DOWN);
        key(VK_RETURN);
        assert_eq!(posted(owner.0), [(COMMAND, 152, 1)]);
        // Escape closes the submenu first, then the menu.
        key(VK_ESCAPE);
        assert_eq!(popup.model.borrow().open, None);
        assert_eq!(posted(owner.0), []);
        key(VK_ESCAPE);
        assert_eq!(posted(owner.0), [(CLOSED, 0, popup.root().0 as isize)]);
    }

    #[test]
    fn losing_the_foreground_closes_the_menu() {
        let owner = owner();
        let popup = hidden(owner.0);
        unsafe {
            SendMessageW(
                popup.root(),
                WM_ACTIVATE,
                Some(WPARAM(WA_INACTIVE as usize)),
                None,
            );
        }
        assert_eq!(posted(owner.0), [(CLOSED, 0, popup.root().0 as isize)]);
    }

    /// Handles what the windows of this thread receive for `ms` milliseconds, and returns
    /// the notices posted to `owner` meanwhile.
    fn pump(owner: HWND, ms: u64) -> Vec<(u32, usize, isize)> {
        let until = std::time::Instant::now() + std::time::Duration::from_millis(ms);
        let mut notices = Vec::new();
        let mut message = MSG::default();
        while std::time::Instant::now() < until {
            unsafe {
                while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                    if message.hwnd == owner && (COMMAND..=CLOSED).contains(&message.message) {
                        notices.push((message.message, message.wParam.0, message.lParam.0));
                        continue;
                    }
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        notices
    }
    fn input(inputs: &[INPUT]) {
        let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
        assert_eq!(sent as usize, inputs.len(), "input refused");
    }

    #[test]
    #[ignore = "real desktop: shows the menu, takes the foreground and clicks it, run alone by check.ps1"]
    fn real_desktop_the_menu_keeps_the_foreground_through_a_setting_and_closes_when_left() {
        let owner = owner();
        let mut pointer = POINT::default();
        unsafe { GetCursorPos(&mut pointer).unwrap() };
        // The last input comes from this process, which may then take the foreground, as the
        // shell lets the resident do on a click on its icon. F24 is on no keyboard.
        let key = |flags| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_F24,
                    dwFlags: flags,
                    ..Default::default()
                },
            },
        };
        input(&[key(KEYBD_EVENT_FLAGS(0)), key(KEYEVENTF_KEYUP)]);
        let mut work = RECT::default();
        unsafe {
            SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                Some((&mut work as *mut RECT).cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
            .unwrap();
        }
        let at = POINT {
            x: (work.left + work.right) / 2,
            y: work.top + 40,
        };
        let popup = Popup::open(owner.0, RECT::default(), Values::default(), at, true).unwrap();
        assert_eq!(pump(owner.0, 200), []);
        let root = popup.root();
        assert_eq!(
            unsafe { GetForegroundWindow() },
            root,
            "the menu has the keyboard"
        );
        // A real click on the halo switch.
        let mut bounds = RECT::default();
        unsafe { GetWindowRect(root, &mut bounds).unwrap() };
        assert_eq!((bounds.left, bounds.top), (at.x, at.y));
        let row = {
            let model = popup.model.borrow();
            let column = &model.columns[0];
            (column.top(index(101)) + column.top(index(101) + 1)) / 2
        };
        unsafe { SetCursorPos(bounds.left + 30, bounds.top + row).unwrap() };
        assert_eq!(pump(owner.0, 50), []);
        let button = |flags| INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dwFlags: flags,
                    ..Default::default()
                },
            },
        };
        input(&[button(MOUSEEVENTF_LEFTDOWN), button(MOUSEEVENTF_LEFTUP)]);
        let clicked = pump(owner.0, 200);
        unsafe { SetCursorPos(pointer.x, pointer.y).unwrap() };
        assert_eq!(clicked, [(COMMAND, 101, 1)]);
        assert!(unsafe { IsWindowVisible(root) }.as_bool());
        assert_eq!(
            unsafe { GetForegroundWindow() },
            root,
            "still in front after the click"
        );
        // Another window taking the foreground stands for a click elsewhere.
        let other = crate::resources::OwnedWindow(unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                w!("STATIC"),
                w!("Elsewhere"),
                WS_POPUP | WS_VISIBLE,
                work.left,
                work.top,
                1,
                1,
                None,
                None,
                None,
                None,
            )
            .unwrap()
        });
        unsafe {
            let _ = SetForegroundWindow(other.0);
        }
        assert_eq!(pump(owner.0, 100), [(CLOSED, 0, root.0 as isize)]);
        assert!(!unsafe { IsWindowVisible(root) }.as_bool());
    }

    #[test]
    fn a_closed_popup_keeps_no_renderer_or_window() {
        let owner = owner();
        let root = {
            let popup = hidden(owner.0);
            assert!(PAINTER.with(|painter| painter.borrow().is_some()));
            popup.root()
        };
        assert!(PAINTER.with(|painter| painter.borrow().is_none()));
        assert!(!unsafe { IsWindow(Some(root)) }.as_bool());
    }
}
