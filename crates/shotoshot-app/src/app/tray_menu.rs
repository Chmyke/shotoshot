//! ROLE: The notification-area menu: one model for its order, native build, painting and preview.
//! OWNER: App builds a menu per opening; the Direct2D painter lives only while one is open.
//! INVARIANTS: Layout is in 96-DPI logical pixels, scaled once at paint time.
use super::*;
use crate::presentation::menu as commands;
use crate::render::{D2dCanvas, Renderer};
use crate::settings::{InputLayout, Shortcut};
use shotoshot_core::{Rect, Tool};
use shotoshot_ui::{Canvas, Ink};
use windows::Win32::UI::Controls::*;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Title,
    Separator,
    /// Runs or opens something; shows its shortcut when it has one.
    Command,
    Toggle,
    /// Opens a submenu of choices and shows the current one.
    Submenu,
    /// One value of a submenu; the current one is ticked.
    Choice,
}

#[derive(Clone, Copy)]
pub(super) enum Icon {
    None,
    Capture,
    Instant,
    Halo,
    Keyboard,
    Mouse,
    Layout,
    Size,
    Clock,
    Sound,
    Power,
    /// A note pinned to the desktop.
    Board,
    /// The editor toolbar's glyph for the same notion.
    Toolbar(Action),
}

struct Row {
    id: u32,
    label: &'static str,
    icon: Icon,
    kind: Kind,
}
const fn row(id: u32, label: &'static str, icon: Icon, kind: Kind) -> Row {
    Row {
        id,
        label,
        icon,
        kind,
    }
}
const fn title(id: u32, label: &'static str) -> Row {
    row(id, label, Icon::None, Kind::Title)
}

/// Command ids are the ones `App::menu` handles; titles (90..) and the separator are inert.
const ROOT: &[Row] = &[
    row(1, "Take a screenshot", Icon::Capture, Kind::Command),
    row(8, "Edit board", Icon::Board, Kind::Command),
    row(9, "Show board", Icon::Board, Kind::Toggle),
    row(
        10,
        "Clear board",
        Icon::Toolbar(Action::ClearBoard),
        Kind::Command,
    ),
    title(90, "Presentation"),
    row(101, "Pointer halo", Icon::Halo, Kind::Toggle),
    row(103, "Keyboard", Icon::Keyboard, Kind::Toggle),
    row(104, "Mouse", Icon::Mouse, Kind::Toggle),
    row(130, "Keyboard layout", Icon::Layout, Kind::Submenu),
    row(140, "Keyboard and mouse size", Icon::Size, Kind::Submenu),
    row(
        102,
        "Temporary shapes",
        Icon::Toolbar(Action::Tool(Tool::Pen)),
        Kind::Toggle,
    ),
    row(120, "Shape duration", Icon::Clock, Kind::Submenu),
    title(91, "Shortcuts · click to change"),
    row(2, "Capture", Icon::Capture, Kind::Command),
    row(6, "Instant screenshot", Icon::Instant, Kind::Command),
    row(
        110,
        "Arrow",
        Icon::Toolbar(Action::Tool(Tool::Arrow)),
        Kind::Command,
    ),
    row(
        111,
        "Rectangle",
        Icon::Toolbar(Action::Tool(Tool::Rectangle)),
        Kind::Command,
    ),
    row(
        112,
        "Ellipse",
        Icon::Toolbar(Action::Tool(Tool::Ellipse)),
        Kind::Command,
    ),
    row(13, "Board", Icon::Board, Kind::Command),
    title(92, "Options"),
    row(7, "Screenshot sound", Icon::Sound, Kind::Toggle),
    row(
        5,
        "Live capture by default",
        Icon::Toolbar(Action::Live),
        Kind::Toggle,
    ),
    row(4, "Start with Windows", Icon::Power, Kind::Toggle),
    row(0, "", Icon::None, Kind::Separator),
    row(
        3,
        "Quit Shotoshot",
        Icon::Toolbar(Action::Close),
        Kind::Command,
    ),
];
const DURATIONS: &[Row] = &[
    row(121, "1 second", Icon::None, Kind::Choice),
    row(122, "3 seconds", Icon::None, Kind::Choice),
    row(123, "5 seconds", Icon::None, Kind::Choice),
];
const LAYOUTS: &[Row] = &[
    row(131, "Full", Icon::None, Kind::Choice),
    row(132, "Compact", Icon::None, Kind::Choice),
    row(133, "Gaming", Icon::None, Kind::Choice),
];
const SIZES: &[Row] = &[
    row(141, "100%", Icon::None, Kind::Choice),
    row(142, "150%", Icon::None, Kind::Choice),
    row(143, "200%", Icon::None, Kind::Choice),
];
/// The values a submenu row opens.
fn choices(id: u32) -> &'static [Row] {
    match id {
        120 => DURATIONS,
        130 => LAYOUTS,
        140 => SIZES,
        _ => &[],
    }
}
fn find(id: u32) -> Option<&'static Row> {
    ROOT.iter()
        .chain(DURATIONS)
        .chain(LAYOUTS)
        .chain(SIZES)
        .find(|row| row.id == id)
}
/// The icon a menu row shows, for the surfaces that continue what the row opens.
pub(super) fn icon(id: u32) -> Icon {
    find(id).map_or(Icon::None, |row| row.icon)
}

/// What the rows display for the current settings.
#[derive(Default)]
pub(super) struct Values {
    /// Packed with `shortcut`.
    pub capture: usize,
    pub instant: usize,
    /// Arrow, rectangle, ellipse.
    pub shapes: [usize; 3],
    pub duration_ms: u32,
    pub layout: InputLayout,
    /// Size of the input diagram, in percent.
    pub size: u32,
    pub halo: bool,
    pub temporary_shapes: bool,
    pub keys: bool,
    pub mouse: bool,
    pub sound: bool,
    pub live: bool,
    pub startup: bool,
    /// Whether the board is shown in the wallpaper.
    pub board: bool,
    /// Packed with `shortcut`.
    pub board_shortcut: usize,
}
impl Values {
    fn checked(&self, id: u32) -> bool {
        match id {
            101 => self.halo,
            102 => self.temporary_shapes,
            103 => self.keys,
            104 => self.mouse,
            7 => self.sound,
            5 => self.live,
            4 => self.startup,
            9 => self.board,
            _ => {
                commands::duration_ms(id) == Some(self.duration_ms)
                    || commands::layout(id) == Some(self.layout)
                    || commands::scale(id) == Some(self.size)
            }
        }
    }
    fn data(&self, id: u32) -> usize {
        match id {
            1 | 2 => self.capture,
            6 => self.instant,
            8 | 13 => self.board_shortcut,
            110..=112 => self.shapes[(id - 110) as usize],
            120 => self.duration_ms as usize,
            130 => self.layout.index(),
            140 => self.size as usize,
            _ => 0,
        }
    }
}

/// Item data of a row showing a shortcut; `unavailable` marks one another program holds.
pub(super) fn shortcut(shortcut: Shortcut, unavailable: bool) -> usize {
    if shortcut.key == 0 {
        return 0;
    }
    ((shortcut.key << 16) | shortcut.modifiers | (u32::from(unavailable) << 31)) as usize
}
fn shortcut_text(data: usize) -> String {
    if data == 0 {
        return String::new();
    }
    format!(
        "{}{}",
        setup::key_label((data & 0xffff) as u32, ((data >> 16) & 0xff) as u32).replace(" + ", "+"),
        if data & (1 << 31) != 0 {
            " (in use)"
        } else {
            ""
        }
    )
}
/// Text on the right of a row.
fn detail(row: &Row, data: usize) -> String {
    match row.kind {
        Kind::Command => shortcut_text(data),
        Kind::Submenu => match row.id {
            130 => LAYOUTS.get(data).map_or("", |layout| layout.label).into(),
            140 => format!("{data}%"),
            _ => format!("{} s", data / 1000),
        },
        _ => String::new(),
    }
}

const BACKGROUND: [u8; 3] = [0x2b, 0x2b, 0x2b];
const HOVER: [u8; 3] = [0x41, 0x41, 0x41];
pub(super) const TEXT: [u8; 3] = [0xf0, 0xf0, 0xf0];
pub(super) const SECONDARY: [u8; 3] = [0xc6, 0xc6, 0xc6];
pub(super) const MUTED: [u8; 3] = [0x9a, 0x9a, 0x9a];
pub(super) const RULE: [u8; 3] = [0x49, 0x49, 0x49];
pub(super) const ACCENT: [u8; 3] = [0x60, 0xcd, 0xff];
const ICON_X: f64 = 12.;
/// Toolbar glyphs sit on a 20 px grid; the menu shows them at 16 px.
const ICON_SCALE: f64 = 0.8;
const LABEL_X: f64 = 38.;
const MARGIN: f64 = 14.;
const TEXT_SIZE: f64 = 13.;
const TITLE_SIZE: f64 = 11.;
const SWITCH: Point = Point { x: 28., y: 14. };
/// Room Windows needs for its own submenu arrow.
const ARROW: f64 = 14.;

fn height(kind: Kind) -> f64 {
    match kind {
        Kind::Title => 26.,
        Kind::Separator => 9.,
        _ => 28.,
    }
}
/// Logical width a row needs to show its label and its right-hand side without clipping.
fn width(renderer: &Renderer, row: &Row, data: usize) -> Result<f64> {
    let label = |text: &str, size: f64| Ok::<_, Error>(renderer.measure_text(text, size as f32)?.x);
    Ok(match row.kind {
        Kind::Separator => 0.,
        Kind::Title => ICON_X + label(row.label, TITLE_SIZE)? + MARGIN,
        Kind::Toggle => LABEL_X + label(row.label, TEXT_SIZE)? + 2. * MARGIN + SWITCH.x + MARGIN,
        Kind::Choice => LABEL_X + label(row.label, TEXT_SIZE)? + 2. * MARGIN,
        Kind::Command | Kind::Submenu => {
            let detail = detail(row, data);
            let arrow = if row.kind == Kind::Submenu { ARROW } else { 0. };
            let right = if detail.is_empty() {
                0.
            } else {
                2. * MARGIN + label(&detail, TEXT_SIZE)?
            };
            LABEL_X + label(row.label, TEXT_SIZE)? + right + arrow + MARGIN
        }
    })
}

#[derive(Clone, Copy, Default)]
struct State {
    selected: bool,
    checked: bool,
    data: usize,
    /// Synthetic previews draw the submenu arrow Windows supplies on a real menu.
    arrow: bool,
}

fn segments<C: Canvas>(
    canvas: &mut C,
    origin: Point,
    scale: f64,
    rgb: [u8; 3],
    points: &[(f64, f64)],
) -> std::result::Result<(), C::Error> {
    let at = |p: (f64, f64)| {
        origin
            + Point {
                x: p.0 * scale,
                y: p.1 * scale,
            }
    };
    for pair in points.windows(2) {
        canvas.line(at(pair[0]), at(pair[1]), Ink::solid(rgb), 1.6 * scale)?;
    }
    Ok(())
}
/// Clockwise arc in degrees from the positive x axis, on the glyph's 20 px grid.
fn arc<C: Canvas>(
    canvas: &mut C,
    origin: Point,
    scale: f64,
    rgb: [u8; 3],
    (center, radius): ((f64, f64), f64),
    (from, to): (f64, f64),
) -> std::result::Result<(), C::Error> {
    let points: Vec<_> = (0..=16)
        .map(|step| {
            let angle = (from + (to - from) * step as f64 / 16.).to_radians();
            (
                center.0 + radius * angle.cos(),
                center.1 + radius * angle.sin(),
            )
        })
        .collect();
    segments(canvas, origin, scale, rgb, &points)
}
pub(super) fn draw_icon<C: Canvas>(
    canvas: &mut C,
    icon: Icon,
    origin: Point,
    scale: f64,
    rgb: [u8; 3],
) -> std::result::Result<(), C::Error> {
    let at = |x: f64, y: f64| {
        origin
            + Point {
                x: x * scale,
                y: y * scale,
            }
    };
    let radius = |r: f64| Point {
        x: r * scale,
        y: r * scale,
    };
    let ink = Ink::solid(rgb);
    match icon {
        Icon::None => {}
        Icon::Toolbar(action) => crate::toolbar::glyph(canvas, action, origin, scale, rgb, rgb)?,
        Icon::Capture => {
            // Corner brackets of a selected area.
            for corner in [
                [(2., 7.), (2., 2.), (7., 2.)],
                [(13., 2.), (18., 2.), (18., 7.)],
                [(18., 13.), (18., 18.), (13., 18.)],
                [(7., 18.), (2., 18.), (2., 13.)],
            ] {
                segments(canvas, origin, scale, rgb, &corner)?;
            }
        }
        Icon::Instant => segments(
            canvas,
            origin,
            scale,
            rgb,
            &[
                (11.5, 1.5),
                (4., 11.5),
                (9.5, 11.5),
                (8.5, 18.5),
                (16., 8.5),
                (10.5, 8.5),
                (11.5, 1.5),
            ],
        )?,
        Icon::Halo => {
            canvas.stroke_ellipse(at(10., 10.), radius(8.), ink, 1.6 * scale)?;
            canvas.fill_ellipse(at(10., 10.), radius(2.8), ink)?;
        }
        Icon::Keyboard => {
            canvas.stroke_round_rect(
                Rect {
                    min: at(1., 4.),
                    max: at(19., 16.),
                },
                2.5 * scale,
                ink,
                1.6 * scale,
            )?;
            for key in [
                [(4.5, 8.5), (6., 8.5)],
                [(9.25, 8.5), (10.75, 8.5)],
                [(14., 8.5), (15.5, 8.5)],
                [(6.5, 12.5), (13.5, 12.5)],
            ] {
                segments(canvas, origin, scale, rgb, &key)?;
            }
        }
        Icon::Mouse => {
            canvas.stroke_round_rect(
                Rect {
                    min: at(5., 1.5),
                    max: at(15., 18.5),
                },
                5. * scale,
                ink,
                1.6 * scale,
            )?;
            segments(canvas, origin, scale, rgb, &[(10., 5.), (10., 8.5)])?;
        }
        Icon::Layout => {
            // A keyboard with its left part set apart.
            canvas.stroke_round_rect(
                Rect {
                    min: at(1., 4.),
                    max: at(19., 16.),
                },
                2.5 * scale,
                ink,
                1.6 * scale,
            )?;
            for stroke in [
                [(11., 4.), (11., 16.)],
                [(4.5, 8.5), (7.5, 8.5)],
                [(4.5, 12.5), (7.5, 12.5)],
            ] {
                segments(canvas, origin, scale, rgb, &stroke)?;
            }
        }
        Icon::Size => {
            // Two corners pulled apart along a diagonal.
            for stroke in [
                &[(11.5, 3.), (17., 3.), (17., 8.5)][..],
                &[(8.5, 17.), (3., 17.), (3., 11.5)],
                &[(16.5, 3.5), (3.5, 16.5)],
            ] {
                segments(canvas, origin, scale, rgb, stroke)?;
            }
        }
        Icon::Clock => {
            canvas.stroke_ellipse(at(10., 10.), radius(8.), ink, 1.6 * scale)?;
            segments(
                canvas,
                origin,
                scale,
                rgb,
                &[(10., 5.5), (10., 10.), (13.5, 12.)],
            )?;
        }
        Icon::Sound => {
            segments(
                canvas,
                origin,
                scale,
                rgb,
                &[
                    (2., 7.5),
                    (5.5, 7.5),
                    (10.5, 3.5),
                    (10.5, 16.5),
                    (5.5, 12.5),
                    (2., 12.5),
                    (2., 7.5),
                ],
            )?;
            arc(canvas, origin, scale, rgb, ((10.5, 10.), 4.), (-45., 45.))?;
            arc(canvas, origin, scale, rgb, ((10.5, 10.), 7.5), (-50., 50.))?;
        }
        Icon::Power => {
            arc(canvas, origin, scale, rgb, ((10., 10.5), 7.5), (-55., 235.))?;
            segments(canvas, origin, scale, rgb, &[(10., 2.), (10., 10.)])?;
        }
        Icon::Board => {
            // A note and two lines of its text.
            canvas.stroke_round_rect(
                Rect {
                    min: at(3., 2.5),
                    max: at(17., 17.5),
                },
                2.5 * scale,
                ink,
                1.6 * scale,
            )?;
            for line in [[(6.5, 8.), (13.5, 8.)], [(6.5, 12.), (11., 12.)]] {
                segments(canvas, origin, scale, rgb, &line)?;
            }
        }
    }
    Ok(())
}

/// One row filling `size` device pixels, at `scale` device pixels per logical pixel.
fn paint<C: Canvas>(
    canvas: &mut C,
    row: &Row,
    state: State,
    size: Point,
    scale: f64,
) -> std::result::Result<(), C::Error> {
    let u = |value: f64| value * scale;
    canvas.fill_rect(
        Rect {
            min: Point::default(),
            max: size,
        },
        Ink::solid(BACKGROUND),
    )?;
    let middle = size.y / 2.;
    let text = |canvas: &mut C, text: &str, left: f64, right: f64, bottom: f64, size: f64, rgb| {
        let line = canvas.measure_text("X", u(size))?.y;
        canvas.text(
            text,
            Rect {
                min: Point {
                    x: left,
                    y: bottom - line,
                },
                max: Point {
                    x: right,
                    y: bottom,
                },
            },
            u(size),
            Ink::solid(rgb),
        )
    };
    match row.kind {
        Kind::Separator => {
            return canvas.fill_rect(
                Rect {
                    min: Point {
                        x: u(ICON_X),
                        y: middle.floor(),
                    },
                    max: Point {
                        x: size.x - u(ICON_X),
                        y: middle.floor() + u(1.).round().max(1.),
                    },
                },
                Ink::solid(RULE),
            );
        }
        Kind::Title => {
            return text(
                canvas,
                row.label,
                u(ICON_X),
                size.x,
                size.y - u(3.),
                TITLE_SIZE,
                MUTED,
            );
        }
        _ => {}
    }
    if state.selected {
        canvas.fill_round_rect(
            Rect {
                min: Point { x: u(4.), y: u(1.) },
                max: Point {
                    x: size.x - u(4.),
                    y: size.y - u(1.),
                },
            },
            u(4.),
            Ink::solid(HOVER),
        )?;
    }
    let icon_origin = Point {
        x: u(ICON_X),
        y: middle - u(10. * ICON_SCALE),
    };
    if row.kind == Kind::Choice {
        if state.checked {
            segments(
                canvas,
                icon_origin,
                u(ICON_SCALE),
                TEXT,
                &[(4., 10.5), (8.5, 15.), (16., 5.5)],
            )?;
        }
    } else {
        draw_icon(canvas, row.icon, icon_origin, u(ICON_SCALE), SECONDARY)?;
    }
    let line = canvas.measure_text("X", u(TEXT_SIZE))?.y;
    let bottom = middle + line / 2.;
    text(
        canvas,
        row.label,
        u(LABEL_X),
        size.x,
        bottom,
        TEXT_SIZE,
        TEXT,
    )?;
    let mut right = size.x - u(MARGIN);
    if row.kind == Kind::Submenu {
        if state.arrow {
            segments(
                canvas,
                Point {
                    x: right - u(8.),
                    y: middle - u(6.),
                },
                u(0.6),
                SECONDARY,
                &[(6., 4.), (12., 10.), (6., 16.)],
            )?;
        }
        right -= u(ARROW);
    }
    if row.kind == Kind::Toggle {
        let track = Rect {
            min: Point {
                x: right - u(SWITCH.x),
                y: middle - u(SWITCH.y / 2.),
            },
            max: Point {
                x: right,
                y: middle + u(SWITCH.y / 2.),
            },
        };
        let knob = |x: f64| Point { x, y: middle };
        if state.checked {
            canvas.fill_round_rect(track, u(SWITCH.y / 2.), Ink::solid(ACCENT))?;
            canvas.fill_ellipse(
                knob(track.max.x - u(SWITCH.y / 2.)),
                Point {
                    x: u(4.5),
                    y: u(4.5),
                },
                Ink::solid(BACKGROUND),
            )?;
        } else {
            let inset = Point {
                x: u(0.5),
                y: u(0.5),
            };
            canvas.stroke_round_rect(
                Rect {
                    min: track.min + inset,
                    max: track.max - inset,
                },
                u(SWITCH.y / 2. - 0.5),
                Ink::solid(MUTED),
                u(1.),
            )?;
            canvas.fill_ellipse(
                knob(track.min.x + u(SWITCH.y / 2.)),
                Point { x: u(4.), y: u(4.) },
                Ink::solid(SECONDARY),
            )?;
        }
        return Ok(());
    }
    let detail = detail(row, state.data);
    if !detail.is_empty() {
        let extent = canvas.measure_text(&detail, u(TEXT_SIZE))?.x;
        text(
            canvas,
            &detail,
            right - extent,
            // Measuring and drawing round differently: never clip the last glyph.
            size.x,
            bottom,
            TEXT_SIZE,
            SECONDARY,
        )?;
    }
    Ok(())
}

thread_local! {
    // Created by the first row of an opening menu and released when it closes:
    // the resident keeps no renderer at rest.
    static PAINTER: RefCell<Option<Renderer>> = const { RefCell::new(None) };
}
/// Releases the painter when the menu closes, including on an early return.
pub(super) struct Open;
impl Drop for Open {
    fn drop(&mut self) {
        PAINTER.with(|painter| painter.borrow_mut().take());
    }
}
fn with_painter<T>(work: impl FnOnce(&Renderer) -> Result<T>) -> Result<T> {
    PAINTER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let renderer = match &mut *slot {
            Some(renderer) => renderer,
            None => slot.insert(Renderer::software()?),
        };
        let result = work(renderer);
        if result.is_err() {
            // A lost render target is rebuilt by the next row.
            *slot = None;
        }
        result
    })
}
fn draw_row(
    renderer: &Renderer,
    dc: HDC,
    bounds: RECT,
    row: &Row,
    state: State,
    scale: f64,
) -> Result<()> {
    unsafe {
        renderer.dc.BindDC(dc, &bounds)?;
        renderer.dc.BeginDraw();
        renderer
            .dc
            .SetTransform(&windows_numerics::Matrix3x2::identity());
        let size = Point {
            x: (bounds.right - bounds.left) as f64,
            y: (bounds.bottom - bounds.top) as f64,
        };
        let painted = D2dCanvas::new(renderer, &renderer.dc)
            .and_then(|mut canvas| paint(&mut canvas, row, state, size, scale));
        let ended = renderer.dc.EndDraw(None, None);
        painted?;
        ended
    }
}

/// Native menu callbacks are reentrant while TrackPopupMenu holds App's borrow.
/// They only measure and draw, so they deliberately need no application state.
pub(super) unsafe fn message(hwnd: HWND, msg: u32, l: LPARAM) -> Option<LRESULT> {
    unsafe {
        let scale = GetDpiForWindow(hwnd).max(96) as f64 / 96.;
        if msg == WM_MEASUREITEM {
            let item = &mut *(l.0 as *mut MEASUREITEMSTRUCT);
            if item.CtlType != ODT_MENU {
                return None;
            }
            let row = find(item.itemID)?;
            let needed = with_painter(|renderer| width(renderer, row, item.itemData)).ok()?;
            item.itemWidth = (needed * scale).ceil() as u32;
            item.itemHeight = (height(row.kind) * scale).round() as u32;
            return Some(LRESULT(1));
        }
        if msg == WM_DRAWITEM {
            let item = &*(l.0 as *const DRAWITEMSTRUCT);
            if item.CtlType != ODT_MENU {
                return None;
            }
            let row = find(item.itemID)?;
            let state = State {
                selected: item.itemState.0 & ODS_SELECTED.0 != 0,
                checked: item.itemState.0 & ODS_CHECKED.0 != 0,
                data: item.itemData,
                arrow: false,
            };
            // Best effort: a failed paint leaves the row blank and the next one retries.
            let _ = with_painter(|renderer| {
                draw_row(renderer, item.hDC, item.rcItem, row, state, scale)
            });
            return Some(LRESULT(1));
        }
        None
    }
}

unsafe fn insert(menu: HMENU, row: &Row, values: &Values, submenu: HMENU) -> Result<()> {
    let inert = matches!(row.kind, Kind::Title | Kind::Separator);
    unsafe {
        InsertMenuItemW(
            menu,
            u32::MAX,
            true,
            &MENUITEMINFOW {
                cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
                fMask: MIIM_FTYPE | MIIM_ID | MIIM_STATE | MIIM_DATA | MIIM_SUBMENU,
                fType: MFT_OWNERDRAW,
                fState: if values.checked(row.id) {
                    MFS_CHECKED
                } else {
                    MFS_UNCHECKED
                } | if inert { MFS_DISABLED } else { MFS_ENABLED },
                wID: row.id,
                hSubMenu: submenu,
                dwItemData: values.data(row.id),
                ..Default::default()
            },
        )
    }
}
/// Fills an empty popup menu. Windows destroys the attached submenus with their parent.
pub(super) fn build(menu: HMENU, values: &Values) -> Result<()> {
    unsafe {
        for row in ROOT {
            if row.kind != Kind::Submenu {
                insert(menu, row, values, HMENU::default())?;
                continue;
            }
            // PITFALL: Attach the submenu before filling it, so the root menu guard owns
            // its cleanup if a later step fails.
            let submenu = CreatePopupMenu()?;
            if let Err(error) = insert(menu, row, values, submenu) {
                let _ = DestroyMenu(submenu);
                return Err(error);
            }
            for choice in choices(row.id) {
                insert(submenu, choice, values, HMENU::default())?;
            }
        }
        Ok(())
    }
}

/// Paints the whole menu, with the submenu of row `open` beside it, at `scale`.
fn render(values: &Values, selected: u32, open: u32, scale: f64) -> Result<Image> {
    let _open = Open;
    with_painter(|renderer| {
        let column = |rows: &[Row]| -> Result<i32> {
            let mut widest: f64 = 0.;
            for row in rows {
                widest = widest.max(width(renderer, row, values.data(row.id))?);
            }
            Ok((widest * scale).ceil() as i32)
        };
        let extent = |rows: &[Row]| -> i32 {
            rows.iter()
                .map(|row| (height(row.kind) * scale).round() as i32)
                .sum()
        };
        let opened = choices(open);
        let (root, beside) = (column(ROOT)?, column(opened)?);
        // The submenu hangs from its row, wherever that row sits in the menu.
        let hang: i32 = ROOT
            .iter()
            .take_while(|row| row.id != open)
            .map(|row| (height(row.kind) * scale).round() as i32)
            .sum();
        paint_image(
            (root + beside) as u32,
            extent(ROOT).max(hang + extent(opened)) as u32,
            |dc| {
                let column = |rows: &[Row], left: i32, mut top: i32, wide: i32| -> Result<i32> {
                    let mut submenu = 0;
                    for row in rows {
                        let bottom = top + (height(row.kind) * scale).round() as i32;
                        if row.id == open {
                            submenu = top;
                        }
                        draw_row(
                            renderer,
                            dc,
                            RECT {
                                left,
                                top,
                                right: left + wide,
                                bottom,
                            },
                            row,
                            State {
                                selected: row.id == selected,
                                checked: values.checked(row.id),
                                data: values.data(row.id),
                                arrow: true,
                            },
                            scale,
                        )?;
                        top = bottom;
                    }
                    Ok(submenu)
                };
                let submenu = column(ROOT, 0, 0, root)?;
                column(opened, root, submenu, beside)?;
                Ok(())
            },
        )
    })
}
fn sample() -> Values {
    let defaults = Settings::default();
    let shapes = defaults.presentation.shape_shortcuts;
    Values {
        capture: shortcut(
            Shortcut {
                modifiers: 0,
                key: crate::print_screen::KEY,
            },
            false,
        ),
        instant: shortcut(defaults.instant_shortcut, false),
        shapes: [
            shortcut(shapes[0], false),
            shortcut(shapes[1], true),
            shortcut(shapes[2], false),
        ],
        duration_ms: defaults.presentation.duration_ms,
        layout: defaults.presentation.input_layout,
        size: defaults.presentation.input_scale,
        halo: true,
        temporary_shapes: true,
        keys: false,
        mouse: false,
        sound: defaults.instant_sound,
        live: defaults.live_capture,
        startup: defaults.start_with_windows,
        board: defaults.board_visible,
        board_shortcut: shortcut(defaults.board_shortcut, false),
    }
}
pub(super) fn preview() -> Result<()> {
    save_png(
        &render(&sample(), 110, 120, 1.)?,
        std::path::Path::new("target/shotoshot-menu.png"),
    )
}
/// Opens and closes the menu's renderer the way a real menu does, for the resident diagnostic.
pub(super) fn probe() -> Result<()> {
    render(&sample(), 0, 120, 1.).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDTH: i32 = 320;
    fn rendered(row: &Row, state: State, scale: f64) -> Image {
        let _open = Open;
        let (width, height) = (
            (WIDTH as f64 * scale) as i32,
            (height(row.kind) * scale).round() as i32,
        );
        paint_image(width as u32, height as u32, |dc| {
            with_painter(|renderer| {
                draw_row(
                    renderer,
                    dc,
                    RECT {
                        right: width,
                        bottom: height,
                        ..Default::default()
                    },
                    row,
                    state,
                    scale,
                )
            })
        })
        .unwrap()
    }
    /// Pixels of logical columns `from..to` that differ from the menu background.
    fn drawn(image: &Image, from: f64, to: f64, scale: f64) -> usize {
        let columns = (from * scale) as usize..(to * scale) as usize;
        image
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter(|(index, pixel)| {
                columns.contains(&(index % image.width as usize))
                    && pixel[..3] != [BACKGROUND[2], BACKGROUND[1], BACKGROUND[0]]
            })
            .count()
    }
    fn accent(image: &Image) -> usize {
        image
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[..3] == [ACCENT[2], ACCENT[1], ACCENT[0]])
            .count()
    }

    #[test]
    fn native_menu_follows_the_model_with_its_submenus() {
        unsafe {
            let menu = CreatePopupMenu().unwrap();
            build(menu, &Values::default()).unwrap();
            let ids: Vec<_> = (0..GetMenuItemCount(Some(menu)))
                .map(|index| {
                    let mut info = MENUITEMINFOW {
                        cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
                        fMask: MIIM_ID,
                        ..Default::default()
                    };
                    GetMenuItemInfoW(menu, index as u32, true, &mut info).unwrap();
                    info.wID
                })
                .collect();
            assert_eq!(ids, ROOT.iter().map(|row| row.id).collect::<Vec<_>>());
            let submenus: Vec<_> = (0..GetMenuItemCount(Some(menu)))
                .filter(|&index| !GetSubMenu(menu, index).is_invalid())
                .collect();
            let opening: Vec<_> = submenus
                .iter()
                .map(|&index| ROOT[index as usize].id)
                .collect();
            assert_eq!(opening, [130, 140, 120]);
            for index in submenus {
                assert_eq!(
                    GetMenuItemCount(Some(GetSubMenu(menu, index))),
                    choices(ROOT[index as usize].id).len() as i32
                );
            }
            DestroyMenu(menu).unwrap();
        }
    }

    #[test]
    fn titles_and_separator_cannot_be_chosen() {
        unsafe {
            let menu = CreatePopupMenu().unwrap();
            build(menu, &Values::default()).unwrap();
            for (index, row) in ROOT.iter().enumerate() {
                let disabled = GetMenuState(menu, index as u32, MF_BYPOSITION) & MF_DISABLED.0 != 0;
                assert_eq!(
                    disabled,
                    matches!(row.kind, Kind::Title | Kind::Separator),
                    "{}",
                    row.label
                );
            }
            DestroyMenu(menu).unwrap();
        }
    }

    #[test]
    fn every_command_and_setting_has_an_icon() {
        for row in ROOT {
            if matches!(row.kind, Kind::Title | Kind::Separator) {
                continue;
            }
            let image = rendered(row, State::default(), 1.);
            assert!(
                drawn(&image, ICON_X, ICON_X + 16., 1.) > 20,
                "{} has no icon",
                row.label
            );
        }
    }

    #[test]
    fn the_board_rows_show_their_state_and_shortcut() {
        let values = Values {
            board: true,
            board_shortcut: 77,
            ..Default::default()
        };
        assert!(values.checked(9));
        assert_eq!([values.data(8), values.data(13)], [77, 77]);
        assert!(find(8).is_some_and(|r| r.label == "Edit board"));
        assert!(find(9).is_some_and(|r| r.label == "Show board" && r.kind == Kind::Toggle));
        assert!(find(13).is_some_and(|r| r.label == "Board" && r.kind == Kind::Command));
        assert!(find(10).is_some_and(|r| r.label == "Clear board"
            && r.kind == Kind::Command
            && matches!(r.icon, Icon::Toolbar(Action::ClearBoard))));
    }

    #[test]
    fn a_switch_shows_whether_a_setting_is_on() {
        let row = find(101).unwrap();
        let off = rendered(row, State::default(), 1.);
        let on = rendered(
            row,
            State {
                checked: true,
                ..Default::default()
            },
            1.,
        );
        let switch = (WIDTH as f64 - MARGIN - SWITCH.x, WIDTH as f64 - MARGIN);
        assert!(
            drawn(&off, switch.0, switch.1, 1.) > 40,
            "an off switch stays visible"
        );
        assert_eq!(accent(&off), 0);
        assert!(accent(&on) > 150);
    }

    #[test]
    fn only_the_current_duration_is_ticked() {
        let row = find(122).unwrap();
        let other = rendered(row, State::default(), 1.);
        let current = rendered(
            row,
            State {
                checked: true,
                ..Default::default()
            },
            1.,
        );
        assert_eq!(drawn(&other, ICON_X, ICON_X + 16., 1.), 0);
        assert!(drawn(&current, ICON_X, ICON_X + 16., 1.) > 15);
        let values = Values {
            duration_ms: 3000,
            layout: InputLayout::Gaming,
            size: 150,
            ..Default::default()
        };
        assert_eq!(
            [121, 122, 123].map(|id| values.checked(id)),
            [false, true, false]
        );
        assert_eq!(
            [131, 132, 133].map(|id| values.checked(id)),
            [false, false, true]
        );
        assert_eq!(
            [141, 142, 143].map(|id| values.checked(id)),
            [false, true, false]
        );
    }

    #[test]
    fn a_submenu_row_names_its_current_choice() {
        let values = Values {
            duration_ms: 5000,
            layout: InputLayout::Compact,
            size: 200,
            ..Default::default()
        };
        let shown = |id| detail(find(id).unwrap(), values.data(id));
        assert_eq!(
            [shown(120), shown(130), shown(140)],
            ["5 s", "Compact", "200%"]
        );
        for row in ROOT.iter().filter(|row| row.kind == Kind::Submenu) {
            assert_eq!(choices(row.id).len(), 3, "{}", row.label);
        }
        assert_eq!(
            LAYOUTS.iter().map(|row| row.label).collect::<Vec<_>>(),
            ["Full", "Compact", "Gaming"]
        );
    }

    #[test]
    fn rows_scale_with_the_monitor() {
        let row = find(4).unwrap();
        let state = State {
            checked: true,
            ..Default::default()
        };
        let (normal, doubled) = (rendered(row, state, 1.), rendered(row, state, 2.));
        assert_eq!((doubled.width, doubled.height), (2 * normal.width, 56));
        // Same layout at twice the size: four times the ink, give or take antialiasing.
        let ratio = accent(&doubled) as f64 / accent(&normal) as f64;
        assert!((3.5..=5.).contains(&ratio), "{ratio}");
        assert_eq!(drawn(&doubled, 0., ICON_X - 1., 2.), 0);
        assert!(drawn(&doubled, ICON_X, ICON_X + 16., 2.) > 80);
    }

    #[test]
    fn width_leaves_room_for_the_shortcut() {
        let _open = Open;
        let row = find(6).unwrap();
        let width_of = |data| with_painter(|renderer| width(renderer, row, data)).unwrap();
        let short = shortcut(
            Shortcut {
                modifiers: 0,
                key: 145,
            },
            false,
        );
        let long = shortcut(
            Shortcut {
                modifiers: 7,
                key: 44,
            },
            true,
        );
        assert_eq!(shortcut_text(short), "Scroll Lock");
        assert_eq!(shortcut_text(long), "Ctrl+Shift+Alt+Print Screen (in use)");
        assert!(width_of(0) < width_of(short) && width_of(short) < width_of(long));
        // The longest shortcut is drawn entirely inside the width the row asks for.
        let needed = width_of(long);
        let image = paint_image(needed.ceil() as u32 + 40, 28, |dc| {
            with_painter(|renderer| {
                draw_row(
                    renderer,
                    dc,
                    RECT {
                        right: needed.ceil() as i32,
                        bottom: 28,
                        ..Default::default()
                    },
                    row,
                    State {
                        data: long,
                        ..Default::default()
                    },
                    1.,
                )
            })
        })
        .unwrap();
        let label_end = LABEL_X
            + with_painter(|renderer| renderer.measure_text(row.label, TEXT_SIZE as f32))
                .unwrap()
                .x;
        assert_eq!(drawn(&image, label_end + 2., label_end + MARGIN, 1.), 0);
        assert_eq!(drawn(&image, needed - MARGIN + 2., needed, 1.), 0);
    }

    #[test]
    fn a_closed_menu_keeps_no_renderer() {
        for open in [120, 130, 140] {
            render(&sample(), 110, open, 1.).unwrap();
        }
        assert!(PAINTER.with(|painter| painter.borrow().is_none()));
    }
}
