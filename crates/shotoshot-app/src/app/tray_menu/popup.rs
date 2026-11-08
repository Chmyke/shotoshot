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
/// Posted to the owner about a shortcut row changed in place: w = row id, l = `EditOp::pack`.
/// The menu cannot check keys: App does, then shows the result with `Popup::set_editing`.
pub(in crate::app) const EDIT: u32 = WM_APP + 18;

/// What happened to a shortcut row changed in place.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::app) enum EditOp {
    /// Starts listening.
    Begin,
    /// A key pressed while listening, its repeats left out.
    KeyDown(u32),
    KeyUp(u32),
    /// Gives the row its keys of a fresh install.
    Default,
    /// Leaves the row as it was.
    Cancel,
    /// Gives the row back its keys from before its last change.
    Undo,
}
impl EditOp {
    pub(in crate::app) fn pack(self) -> isize {
        let (op, key) = match self {
            Self::Begin => (0, 0),
            Self::KeyDown(key) => (1, key),
            Self::KeyUp(key) => (2, key),
            Self::Default => (3, 0),
            Self::Cancel => (4, 0),
            Self::Undo => (5, 0),
        };
        (op | (key << 8)) as isize
    }
    pub(in crate::app) fn unpack(l: isize) -> Option<Self> {
        let key = (l >> 8) as u32;
        Some(match l & 0xff {
            0 => Self::Begin,
            1 => Self::KeyDown(key),
            2 => Self::KeyUp(key),
            3 => Self::Default,
            4 => Self::Cancel,
            5 => Self::Undo,
            _ => return None,
        })
    }
}
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

/// A shortcut row changing in place, and the row that offers its keys from before back.
#[derive(Clone, Default)]
pub(super) struct View {
    pub(super) editing: Option<RowEdit>,
    pub(super) before: Option<u32>,
    /// The shortcut row under the pointer, and whether the pointer is on its keys.
    pub(super) pointer: Option<(u32, bool)>,
}
impl View {
    fn look(&self, id: u32) -> Look<'_> {
        match &self.editing {
            Some(edit) if row_id(edit.row) == Some(id) => Look::Editing(edit),
            Some(_) => Look::Dimmed,
            None if self.before == Some(id) => Look::Undo,
            None => Look::Plain,
        }
    }
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
            tops.push(top + (height(row) * scale).round() as i32);
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
        view: &View,
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
                        look: view.look(row.id),
                        zone: match view.pointer {
                            Some((id, keys)) if id == row.id => {
                                if keys {
                                    Zone::Keys
                                } else {
                                    Zone::Label
                                }
                            }
                            _ => Zone::Row,
                        },
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
    view: View,
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
    fn post_edit(&self, id: u32, op: EditOp) {
        self.post(EDIT, id as usize, op.pack());
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
        // The keyboard lights whole rows.
        self.view.pointer = None;
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
    /// The size of root row `index`, in logical pixels.
    fn row_size(&self, index: usize) -> Point {
        let column = &self.columns[0];
        Point {
            x: column.width as f64 / self.scale,
            y: (column.top(index + 1) - column.top(index)) as f64 / self.scale,
        }
    }
    /// Whether `x`, in logical pixels, is on the keys of root row `index`, a shortcut row.
    fn on_keys(&self, index: usize, x: f64) -> Result<bool> {
        let id = ROOT[index].id;
        if shortcut_row(id).is_none() {
            return Ok(false);
        }
        let caps =
            with_painter(|renderer| caps_width_in(renderer, self.values.data(id), language()))?;
        Ok(x >= keys_left(self.row_size(index).x, caps))
    }
    /// The pointer moved to `x`, `y` of `column`: a shortcut row lights its label or its keys.
    fn point(&mut self, column: usize, x: i32, y: i32) -> Result<()> {
        let pointer = match self.columns[column].at(y) {
            Some(index) if column == 0 && shortcut_row(ROOT[index].id).is_some() => {
                Some((ROOT[index].id, self.on_keys(index, x as f64 / self.scale)?))
            }
            _ => None,
        };
        if pointer != self.view.pointer {
            self.view.pointer = pointer;
            self.redraw(0);
        }
        Ok(())
    }
    /// A release at `x`, in logical pixels, on root row `index`, when it changes a shortcut:
    /// on its undo button or on its keys.
    fn shortcut_click(&self, index: usize, x: f64) -> Result<Option<EditOp>> {
        let id = ROOT[index].id;
        if shortcut_row(id).is_none() {
            return Ok(None);
        }
        let size = self.row_size(index);
        let caps =
            with_painter(|renderer| caps_width_in(renderer, self.values.data(id), language()))?;
        let undo = undo_part(size, caps);
        if self.view.before == Some(id) && (undo.min.x..undo.max.x).contains(&x) {
            return Ok(Some(EditOp::Undo));
        }
        Ok((x >= keys_left(size.x, caps)).then_some(EditOp::Begin))
    }
    /// A release at `x`, `y` while a row listens: its buttons act, anywhere else leaves it.
    fn editing_click(&self, edit: &RowEdit, column: usize, x: f64, y: i32) -> Result<()> {
        let Some(id) = row_id(edit.row) else {
            return Ok(());
        };
        let index = self.columns[column].at(y);
        if column == 1 || index.is_none_or(|index| ROOT[index].id != id) {
            self.post_edit(id, EditOp::Cancel);
            return Ok(());
        }
        let size = self.row_size(index.unwrap_or_default());
        let extent = with_painter(|renderer| {
            Ok(renderer
                .measure_styled_text_in(
                    tr_in(language(), Key::MenuPressKeys),
                    settings_panel::SMALL_SIZE as f32,
                    TextStyle::Body,
                    language(),
                )?
                .x)
        })?;
        let [_, default, cancel] = edit_parts(size, slot_width(extent));
        if (default.min.x..default.max.x).contains(&x) {
            self.post_edit(id, EditOp::Default);
        } else if (cancel.min.x..cancel.max.x).contains(&x) {
            self.post_edit(id, EditOp::Cancel);
        }
        Ok(())
    }
    fn released(&mut self, column: usize, x: i32, y: i32) -> Result<()> {
        let x = x as f64 / self.scale;
        if let Some(edit) = &self.view.editing {
            return self.editing_click(edit, column, x, y);
        }
        let Some(index) = self.columns[column].at(y) else {
            return Ok(());
        };
        if column == 0
            && let Some(op) = self.shortcut_click(index, x)?
        {
            self.post_edit(ROOT[index].id, op);
            return Ok(());
        }
        self.activate(column, index, false)
    }
    /// A key while a row listens: every key goes to it, Esc leaves it and Backspace gives the
    /// default keys; a key held down repeats, and only its first press counts.
    fn editing_key(&self, edit: &RowEdit, msg: u32, key: u32, repeat: bool) {
        let Some(id) = row_id(edit.row) else {
            return;
        };
        let op = match msg {
            WM_KEYUP | WM_SYSKEYUP => EditOp::KeyUp(key),
            _ if repeat => return,
            _ if key == VK_ESCAPE.0 as u32 => EditOp::Cancel,
            _ if key == VK_BACK.0 as u32 => EditOp::Default,
            _ => EditOp::KeyDown(key),
        };
        self.post_edit(id, op);
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
                &self.view,
                self.scale,
            );
            let _ = EndPaint(hwnd, &paint);
            painted
        }
    }
    fn handle(&mut self, hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> Result<Option<LRESULT>> {
        let column = usize::from(hwnd == self.sub);
        let y = (l.0 >> 16) as u16 as i16 as i32;
        let x = l.0 as u16 as i16 as i32;
        if let Some(edit) = &self.view.editing {
            match msg {
                // Alt combinations included: the system must not take them for its menu.
                WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP => {
                    self.editing_key(edit, msg, w.0 as u32, l.0 & (1 << 30) != 0);
                    return Ok(Some(LRESULT(0)));
                }
                WM_CHAR | WM_SYSCHAR => return Ok(Some(LRESULT(0))),
                // The rows keep still while one listens.
                WM_MOUSEMOVE | WM_TIMER => return Ok(Some(LRESULT(0))),
                _ => {}
            }
        }
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
                self.point(column, x, y)?;
            }
            WM_MOUSELEAVE => self.left(column),
            // Released, as in a native menu, with either button.
            WM_LBUTTONUP | WM_RBUTTONUP => self.released(column, x, y)?,
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
                view: View::default(),
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
    /// Shows `editing` in its row, or no row listening; the other rows dim while one listens.
    pub(in crate::app) fn set_editing(&self, editing: Option<RowEdit>) {
        let mut model = self.model.borrow_mut();
        model.view.editing = editing;
        model.redraw(0);
    }
    /// The row listening, as the menu shows it.
    pub(in crate::app) fn editing(&self) -> Option<RowEdit> {
        self.model.borrow().view.editing.clone()
    }
    /// Shows the undo button of `row`, whose shortcut just changed; None hides it.
    pub(in crate::app) fn set_before(&self, row: Option<ShortcutRow>) {
        let mut model = self.model.borrow_mut();
        model.view.before = row.and_then(row_id);
        model.redraw(0);
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
mod tests;
