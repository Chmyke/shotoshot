//! ROLE: The notification-area menu: one model for its order, painting and preview.
//! OWNER: App opens a `Popup` per opening; the Direct2D painter lives only while one is open.
//! INVARIANTS: Layout is in 96-DPI logical pixels, scaled once at paint time.
use super::*;
use crate::render::{D2dCanvas, Renderer};
use crate::settings::Shortcut;
use shotoshot_core::{Rect, Tool};
use shotoshot_text::{Key, Language, LanguageChoice, language, tr_in};
pub(super) use shotoshot_ui::icon::Icon;
pub(super) use shotoshot_ui::icon::draw as draw_icon;
use shotoshot_ui::{Canvas, Ink, TextStyle, settings_panel};
use std::borrow::Cow;
mod popup;
pub(super) use popup::{CLOSED, COMMAND, Popup};

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

/// What a row shows as its name.
#[derive(Clone, Copy)]
enum Label {
    Text(Key),
    /// The language submenu: its word in the interface language, then the English one.
    Languages,
    /// A language under its own name, whatever the interface language.
    Native(Language),
    Blank,
}

#[derive(Clone, Copy)]
struct Row {
    id: u32,
    label: Label,
    icon: Icon,
    kind: Kind,
}
const fn row(id: u32, label: Key, icon: Icon, kind: Kind) -> Row {
    Row {
        id,
        label: Label::Text(label),
        icon,
        kind,
    }
}
const fn title(id: u32, label: Key) -> Row {
    row(id, label, Icon::None, Kind::Title)
}
const fn choice(id: u32, label: Label) -> Row {
    Row {
        id,
        label,
        icon: Icon::None,
        kind: Kind::Choice,
    }
}

/// Command ids are the ones `App::menu` handles; titles (90..) and the separator are inert.
/// The menu acts; what is set once lives in the Settings window (Mickaël, 2026-10-30).
const ROOT: &[Row] = &[
    row(1, Key::MenuTakeScreenshot, Icon::Capture, Kind::Command),
    row(7, Key::MenuInstantScreenshot, Icon::Instant, Kind::Command),
    title(91, Key::MenuBoard),
    row(8, Key::MenuEditBoard, Icon::BoardEdit, Kind::Command),
    row(9, Key::MenuShowBoard, Icon::Eye, Kind::Toggle),
    row(
        10,
        Key::MenuClearBoard,
        Icon::Toolbar(Action::ClearBoard),
        Kind::Command,
    ),
    title(90, Key::MenuPresentation),
    row(101, Key::MenuPointerEffects, Icon::Halo, Kind::Toggle),
    row(107, Key::MenuMagnifier, Icon::Loupe, Kind::Toggle),
    row(103, Key::MenuKeyboard, Icon::Keyboard, Kind::Toggle),
    row(104, Key::MenuMouse, Icon::Mouse, Kind::Toggle),
    row(
        102,
        Key::MenuTemporaryShapes,
        Icon::Toolbar(Action::Tool(Tool::Pen)),
        Kind::Toggle,
    ),
    // Their keys, to learn them; a click records new ones (Mickaël, 2026-10-30).
    row(
        171,
        Key::ToolbarArrow,
        Icon::Toolbar(Action::Tool(Tool::Arrow)),
        Kind::Command,
    ),
    row(
        172,
        Key::ToolbarRectangle,
        Icon::Toolbar(Action::Tool(Tool::Rectangle)),
        Kind::Command,
    ),
    row(
        173,
        Key::ToolbarEllipse,
        Icon::Toolbar(Action::Tool(Tool::Ellipse)),
        Kind::Command,
    ),
    Row {
        id: 0,
        label: Label::Blank,
        icon: Icon::None,
        kind: Kind::Separator,
    },
    row(170, Key::SettingsTitle, Icon::Gear, Kind::Command),
    // Right above Quit: a fixed place, found without reading the menu.
    Row {
        id: 150,
        label: Label::Languages,
        icon: Icon::Globe,
        kind: Kind::Submenu,
    },
    row(3, Key::MenuQuit, Icon::Power, Kind::Command),
];
/// The automatic choice (151), then every language in table order (152..=166).
const LANGUAGES: [Row; 1 + Language::ALL.len()] = {
    let mut rows = [choice(151, Label::Text(Key::MenuLanguageAuto)); 1 + Language::ALL.len()];
    let mut index = 0;
    while index < Language::ALL.len() {
        rows[index + 1] = choice(152 + index as u32, Label::Native(Language::ALL[index]));
        index += 1;
    }
    rows
};
/// The values a submenu row opens.
fn choices(id: u32) -> &'static [Row] {
    match id {
        150 => &LANGUAGES,
        _ => &[],
    }
}
#[cfg(test)]
fn find(id: u32) -> Option<&'static Row> {
    ROOT.iter().chain(&LANGUAGES).find(|row| row.id == id)
}
/// The language a choice of the language submenu selects, None for any other command.
pub(super) fn on_language_choice(id: u32) -> Option<LanguageChoice> {
    let index = id.checked_sub(151)?;
    LanguageChoice::ALL.get(index as usize).copied()
}
/// The name of a row in `language`.
fn label_in(row: &Row, language: Language) -> Cow<'static, str> {
    match row.label {
        Label::Text(key) => tr_in(language, key).into(),
        Label::Languages => shotoshot_text::language_menu_label_in(language),
        Label::Native(name) => name.native_name().into(),
        Label::Blank => "".into(),
    }
}
fn label(row: &Row) -> Cow<'static, str> {
    label_in(row, language())
}

/// What the rows display for the current settings.
#[derive(Clone, Default)]
pub(super) struct Values {
    /// Packed with `shortcut`.
    pub capture: usize,
    pub instant: usize,
    pub halo: bool,
    pub loupe: bool,
    pub temporary_shapes: bool,
    /// The arrow, rectangle and ellipse shortcuts, packed with `shortcut`.
    pub shapes: [usize; 3],
    pub keys: bool,
    pub mouse: bool,
    /// Whether the board is shown in the wallpaper.
    pub board: bool,
    /// Packed with `shortcut`.
    pub board_shortcut: usize,
    pub language: LanguageChoice,
}
impl Values {
    fn checked(&self, id: u32) -> bool {
        if let Some(choice) = on_language_choice(id) {
            return choice == self.language;
        }
        match id {
            101 => self.halo,
            107 => self.loupe,
            102 => self.temporary_shapes,
            103 => self.keys,
            104 => self.mouse,
            9 => self.board,
            _ => false,
        }
    }
    fn data(&self, id: u32) -> usize {
        match id {
            1 => self.capture,
            7 => self.instant,
            8 => self.board_shortcut,
            171..=173 => self.shapes[id as usize - 171],
            // The position of the current choice in its submenu.
            150 => LANGUAGES
                .iter()
                .position(|row| on_language_choice(row.id) == Some(self.language))
                .unwrap_or(0),
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
/// The keys of a shortcut row in `language`, one keycap each, and whether another program
/// holds them: the keycaps then turn orange.
fn caps_in(data: usize, language: Language) -> (Vec<String>, bool) {
    if data == 0 {
        return (Vec::new(), false);
    }
    let (mods, key) = ((data & 0xffff) as u32, ((data >> 16) & 0xff) as u32);
    let caps = setup::key_label_in(mods, key, language)
        .split(" + ")
        .map(str::to_string)
        .collect();
    (caps, data & (1 << 31) != 0)
}
/// The current choice a submenu row names on its right, in `language`.
fn detail_in(row: &Row, data: usize, language: Language) -> String {
    match row.kind {
        Kind::Submenu => choices(row.id)
            .get(data)
            .map_or(String::new(), |current| label_in(current, language).into()),
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
// The type of the Settings window (Mickaël, 2026-10-30): group titles semibold.
const TEXT_SIZE: f64 = 14.;
const TITLE_SIZE: f64 = 12.;
const SWITCH: Point = Point { x: 28., y: 14. };
/// Room for the submenu arrow.
const ARROW: f64 = 14.;

fn height(kind: Kind) -> f64 {
    match kind {
        Kind::Title => 28.,
        Kind::Separator => 9.,
        _ => 32.,
    }
}
/// Group titles are the only text at `TITLE_SIZE`, and the only semibold one.
fn style(size: f64) -> TextStyle {
    if size == TITLE_SIZE {
        TextStyle::Strong
    } else {
        TextStyle::Body
    }
}
/// Logical width a row needs to show its label and its right-hand side without clipping.
fn width(renderer: &Renderer, row: &Row, data: usize) -> Result<f64> {
    width_in(renderer, row, data, language())
}
/// `width` for the texts of `language`.
fn width_in(renderer: &Renderer, row: &Row, data: usize, language: Language) -> Result<f64> {
    let measure = |text: &str, size: f64| {
        Ok::<_, Error>(
            renderer
                .measure_styled_text_in(text, size as f32, style(size), language)?
                .x,
        )
    };
    let name = label_in(row, language);
    Ok(match row.kind {
        Kind::Separator => 0.,
        Kind::Title => ICON_X + measure(&name, TITLE_SIZE)? + MARGIN,
        Kind::Toggle => LABEL_X + measure(&name, TEXT_SIZE)? + 2. * MARGIN + SWITCH.x + MARGIN,
        Kind::Choice => LABEL_X + measure(&name, TEXT_SIZE)? + 2. * MARGIN,
        Kind::Command => {
            let (caps, _) = caps_in(data, language);
            let right = if caps.is_empty() {
                0.
            } else {
                let mut widths = Vec::with_capacity(caps.len());
                for cap in &caps {
                    widths.push(measure(cap, settings_panel::SMALL_SIZE)?);
                }
                2. * MARGIN + settings_panel::keycaps_width(widths)
            };
            LABEL_X + measure(&name, TEXT_SIZE)? + right + MARGIN
        }
        Kind::Submenu => {
            let detail = detail_in(row, data, language);
            let right = if detail.is_empty() {
                0.
            } else {
                2. * MARGIN + measure(&detail, TEXT_SIZE)?
            };
            LABEL_X + measure(&name, TEXT_SIZE)? + right + ARROW + MARGIN
        }
    })
}

#[derive(Clone, Copy, Default)]
struct State {
    selected: bool,
    checked: bool,
    data: usize,
}

/// The glyph `row` shows: Show board's eye is struck through while the board is hidden.
fn shown_icon(row: &Row, state: State) -> Icon {
    match row.icon {
        Icon::Eye if !state.checked => Icon::EyeOff,
        icon => icon,
    }
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
    let name = label(row);
    let middle = size.y / 2.;
    let text = |canvas: &mut C, text: &str, left: f64, right: f64, bottom: f64, size: f64, rgb| {
        let line = canvas.measure_styled_text("X", u(size), style(size))?.y;
        canvas.styled_text(
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
            style(size),
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
                &name,
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
            draw_icon(canvas, Icon::Check, icon_origin, u(ICON_SCALE), TEXT)?;
        }
    } else {
        draw_icon(
            canvas,
            shown_icon(row, state),
            icon_origin,
            u(ICON_SCALE),
            SECONDARY,
        )?;
    }
    let line = canvas
        .measure_styled_text("X", u(TEXT_SIZE), TextStyle::Body)?
        .y;
    let bottom = middle + line / 2.;
    text(canvas, &name, u(LABEL_X), size.x, bottom, TEXT_SIZE, TEXT)?;
    let mut right = size.x - u(MARGIN);
    if row.kind == Kind::Submenu {
        draw_icon(
            canvas,
            Icon::Chevron,
            Point {
                x: right - u(8.),
                y: middle - u(6.),
            },
            u(0.6),
            SECONDARY,
        )?;
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
    if row.kind == Kind::Command {
        let (caps, in_use) = caps_in(state.data, language());
        return settings_panel::keycaps(canvas, &caps, right, middle, 9., scale, in_use);
    }
    let detail = detail_in(row, state.data, language());
    if !detail.is_empty() {
        let extent = canvas
            .measure_styled_text(&detail, u(TEXT_SIZE), TextStyle::Body)?
            .x;
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

/// Paints the whole menu, with the submenu of row `open` beside it, at `scale`.
fn render(values: &Values, selected: u32, open: u32, scale: f64) -> Result<Image> {
    let _open = Open;
    let root = popup::Column::new(ROOT, values, scale)?;
    let beside = popup::Column::new(choices(open), values, scale)?;
    let position = |id: u32| ROOT.iter().position(|row| row.id == id);
    // The submenu's first row lines up with the row that opens it, as on screen.
    let hang = position(open).map_or(0, |index| root.top(index) - beside.top(0));
    paint_image(
        (root.width + beside.width) as u32,
        root.height().max(hang + beside.height()) as u32,
        |dc| {
            root.paint(dc, POINT::default(), values, position(selected), scale)?;
            beside.paint(
                dc,
                POINT {
                    x: root.width,
                    y: hang,
                },
                values,
                None,
                scale,
            )
        },
    )
}
fn sample() -> Values {
    let defaults = Settings::default();
    Values {
        capture: shortcut(
            Shortcut {
                modifiers: 0,
                key: crate::print_screen::KEY,
            },
            false,
        ),
        instant: shortcut(defaults.instant_shortcut, false),
        halo: true,
        loupe: false,
        temporary_shapes: true,
        shapes: defaults
            .presentation
            .shape_shortcuts
            .map(|keys| shortcut(keys, false)),
        keys: false,
        mouse: false,
        board: defaults.board_visible,
        board_shortcut: shortcut(defaults.board_shortcut, false),
        language: defaults.language,
    }
}
pub(super) fn preview() -> Result<()> {
    save_png(
        &render(&sample(), 170, 0, 1.)?,
        std::path::Path::new("target/shotoshot-menu.png"),
    )
}
/// Opens and closes the menu's renderer the way a real menu does, for the resident diagnostic.
pub(super) fn probe() -> Result<()> {
    render(&sample(), 0, 150, 1.).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The English text of a row, as the existing assertions compare it.
    fn english(row: &Row) -> String {
        label_in(row, Language::En).into_owned()
    }

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
    fn every_command_and_setting_has_an_icon() {
        for row in ROOT {
            if matches!(row.kind, Kind::Title | Kind::Separator) {
                continue;
            }
            let image = rendered(row, State::default(), 1.);
            assert!(
                drawn(&image, ICON_X, ICON_X + 16., 1.) > 20,
                "{} has no icon",
                english(row)
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
        assert_eq!(values.data(8), 77);
        assert!(find(8).is_some_and(|r| english(r) == "Edit board"));
        assert!(find(9).is_some_and(|r| english(r) == "Show board" && r.kind == Kind::Toggle));
        assert!(find(10).is_some_and(|r| english(r) == "Clear board"
            && r.kind == Kind::Command
            && matches!(r.icon, Icon::Toolbar(Action::ClearBoard))));
    }

    #[test]
    fn the_shape_rows_teach_their_shortcuts() {
        let values = Values {
            shapes: [11, 22, 33],
            ..Default::default()
        };
        // Right under the switch they need, each with its keys; a click records new ones.
        let at = |id| ROOT.iter().position(|row| row.id == id).unwrap();
        assert_eq!(
            [at(171), at(172), at(173)],
            [at(102) + 1, at(102) + 2, at(102) + 3]
        );
        for (id, keys) in [(171, 11), (172, 22), (173, 33)] {
            assert!(find(id).is_some_and(|row| row.kind == Kind::Command));
            assert_eq!(values.data(id), keys);
        }
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
    fn the_menu_acts_and_settings_open_elsewhere() {
        let actions = ROOT
            .iter()
            .filter(|row| !matches!(row.kind, Kind::Title | Kind::Separator))
            .count();
        assert_eq!(actions, 16, "a short menu");
        assert!(find(170).is_some_and(|r| english(r) == "Settings" && r.kind == Kind::Command));
        // One submenu only: the language, the way back for who cannot read the current one.
        let submenus: Vec<u32> = ROOT
            .iter()
            .filter(|row| row.kind == Kind::Submenu)
            .map(|row| row.id)
            .collect();
        assert_eq!(submenus, [150]);
        // Each board row has its own icon.
        assert!(matches!(find(8).map(|r| r.icon), Some(Icon::BoardEdit)));
        assert!(matches!(find(9).map(|r| r.icon), Some(Icon::Eye)));
    }

    #[test]
    fn each_command_shows_its_own_icon() {
        // Mickaël, 2026-10-30: the power symbol quits, a bin clears the board, a pencil edits
        // it and the eye of Show board is struck through while the board is hidden.
        let icon = |id: u32| find(id).map(|row| row.icon);
        assert_eq!(icon(3), Some(Icon::Power));
        assert_eq!(icon(10), Some(Icon::Toolbar(Action::ClearBoard)));
        assert_eq!(icon(8), Some(Icon::BoardEdit));
        let show = find(9).unwrap();
        let shown = |checked| {
            shown_icon(
                show,
                State {
                    checked,
                    ..Default::default()
                },
            )
        };
        assert_eq!(shown(true), Icon::Eye);
        assert_eq!(shown(false), Icon::EyeOff);
        // No two rows of the menu share a glyph.
        let icons: Vec<Icon> = ROOT
            .iter()
            .filter(|row| !matches!(row.kind, Kind::Title | Kind::Separator))
            .map(|row| row.icon)
            .collect();
        for (index, a) in icons.iter().enumerate() {
            assert!(*a != Icon::None, "a row without an icon");
            assert!(!icons[index + 1..].contains(a), "{a:?} is used by two rows");
        }
        // The board bin is not the eraser of one object.
        assert_ne!(icon(10), Some(Icon::Toolbar(Action::Tool(Tool::Eraser))));
    }

    #[test]
    fn rows_scale_with_the_monitor() {
        let row = find(101).unwrap();
        let state = State {
            checked: true,
            ..Default::default()
        };
        let (normal, doubled) = (rendered(row, state, 1.), rendered(row, state, 2.));
        assert_eq!(
            (doubled.width, doubled.height),
            (2 * normal.width, 2 * height(Kind::Toggle) as u32)
        );
        // Same layout at twice the size: four times the ink, give or take antialiasing.
        let ratio = accent(&doubled) as f64 / accent(&normal) as f64;
        assert!((3.5..=5.).contains(&ratio), "{ratio}");
        assert_eq!(drawn(&doubled, 0., ICON_X - 1., 2.), 0);
        assert!(drawn(&doubled, ICON_X, ICON_X + 16., 2.) > 80);
    }

    #[test]
    fn width_leaves_room_for_the_shortcut() {
        let _open = Open;
        let row = find(7).unwrap();
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
        assert_eq!(
            caps_in(short, Language::En),
            (vec!["Scroll Lock".into()], false)
        );
        assert_eq!(
            caps_in(long, Language::En),
            (
                ["Ctrl", "Shift", "Alt", "Print Screen"]
                    .map(String::from)
                    .to_vec(),
                true
            )
        );
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
            + with_painter(|renderer| {
                renderer.measure_styled_text(&label(row), TEXT_SIZE as f32, TextStyle::Body)
            })
            .unwrap()
            .x;
        assert_eq!(drawn(&image, label_end + 2., label_end + MARGIN, 1.), 0);
        assert_eq!(drawn(&image, needed - MARGIN + 2., needed, 1.), 0);
    }

    #[test]
    fn english_labels_are_unchanged() {
        let root: Vec<_> = ROOT
            .iter()
            .filter(|row| row.id != 150)
            .map(english)
            .collect();
        assert_eq!(
            root,
            [
                "Take a screenshot",
                "Instant screenshot",
                "Board",
                "Edit board",
                "Show board",
                "Clear board",
                "Presentation",
                "Pointer effects",
                "Magnifier",
                "Keyboard",
                "Mouse",
                "Temporary shapes",
                "Arrow",
                "Rectangle",
                "Ellipse",
                "",
                "Settings",
                "Quit Shotoshot",
            ]
        );
    }

    #[test]
    fn language_row_keeps_the_english_word() {
        let row = find(150).unwrap();
        assert!(row.kind == Kind::Submenu && matches!(row.icon, Icon::Globe));
        assert_eq!(label_in(row, Language::En), "Language");
        assert_eq!(label_in(row, Language::Ko), "언어 · Language");
        for language in Language::ALL.into_iter().skip(1) {
            let label = label_in(row, language);
            assert!(
                label.ends_with(" · Language"),
                "{}: {label}",
                language.tag()
            );
            assert!(
                !label.starts_with("Language"),
                "{}: {label}",
                language.tag()
            );
        }
    }

    #[test]
    fn language_choices_show_native_names() {
        let expected = [
            "English",
            "简体中文",
            "Русский",
            "Español",
            "Português (Brasil)",
            "Deutsch",
            "日本語",
            "Français",
            "Polski",
            "한국어",
            "繁體中文",
            "Türkçe",
            "Українська",
            "Italiano",
            "Čeština",
        ];
        for language in Language::ALL {
            let names: Vec<_> = (152..=166)
                .map(|id| label_in(find(id).unwrap(), language))
                .collect();
            assert_eq!(names, expected, "{}", language.tag());
        }
        assert_eq!(
            label_in(find(151).unwrap(), Language::En),
            "Windows language"
        );
        assert_eq!(choices(150).len(), 16);
        assert!(choices(150).iter().all(|row| row.kind == Kind::Choice));
    }

    #[test]
    fn checked_language_follows_the_setting() {
        let ticked = |values: &Values| {
            (150..=167)
                .filter(|&id| values.checked(id))
                .collect::<Vec<_>>()
        };
        assert_eq!(ticked(&Values::default()), [151]);
        let german = Values {
            language: LanguageChoice::Fixed(Language::De),
            ..Default::default()
        };
        assert_eq!(ticked(&german), [157]);
        // The row names the current choice, like the other submenus.
        let row = find(150).unwrap();
        assert_eq!(detail_in(row, german.data(150), Language::En), "Deutsch");
        assert_eq!(
            detail_in(row, Values::default().data(150), Language::En),
            "Windows language"
        );
    }

    #[test]
    fn a_language_choice_is_read_from_its_id() {
        assert_eq!(on_language_choice(151), Some(LanguageChoice::Auto));
        assert_eq!(
            on_language_choice(152),
            Some(LanguageChoice::Fixed(Language::En))
        );
        assert_eq!(
            on_language_choice(157),
            Some(LanguageChoice::Fixed(Language::De))
        );
        assert_eq!(
            on_language_choice(166),
            Some(LanguageChoice::Fixed(Language::Cs))
        );
        for id in [0, 3, 143, 150, 167] {
            assert_eq!(on_language_choice(id), None, "{id}");
        }
        for (index, row) in choices(150).iter().enumerate().skip(1) {
            assert_eq!(
                on_language_choice(row.id),
                Some(LanguageChoice::Fixed(Language::ALL[index - 1]))
            );
        }
    }

    /// Widest row of a menu column in `language`, in logical pixels at scale 1.
    fn widest(rows: &[Row], values: &Values, language: Language) -> f64 {
        with_painter(|renderer| {
            let mut widest: f64 = 0.;
            for row in rows {
                // The language row names any of its sixteen choices.
                let data: Vec<usize> = if row.id == 150 {
                    (0..choices(150).len()).collect()
                } else {
                    vec![values.data(row.id)]
                };
                for data in data {
                    widest = widest.max(width_in(renderer, row, data, language)?);
                }
            }
            Ok(widest)
        })
        .unwrap()
    }

    #[test]
    fn every_menu_fits() {
        let _open = Open;
        let values = sample();
        for language in Language::ALL {
            for rows in [ROOT, &LANGUAGES[..]] {
                let width = widest(rows, &values, language);
                assert!(
                    width <= 480.,
                    "{}: a menu is {width:.0} px wide; shorten its longest text",
                    language.tag()
                );
            }
        }
    }

    #[test]
    fn a_closed_menu_keeps_no_renderer() {
        for open in [0, 150] {
            render(&sample(), 170, open, 1.).unwrap();
        }
        assert!(PAINTER.with(|painter| painter.borrow().is_none()));
    }
}
