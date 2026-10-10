//! ROLE: The shortcut window: what it records, how it is painted and where it is clicked.
//! OWNER: App holds one `Setup` per opening; its painter lives only while the window is shown.
//! INVARIANTS: Layout is in 96-DPI logical pixels, scaled once at paint time.
use super::*;
use crate::render::{D2dCanvas, Renderer};
use shotoshot_core::Rect;
use shotoshot_ui::{Canvas, Ink};
use tray_menu::{ACCENT, MUTED, RULE, SECONDARY, TEXT};

pub(super) const WIDTH: i32 = 420;
pub(super) const HEIGHT: i32 = 248;
const fn area(left: f64, top: f64, right: f64, bottom: f64) -> Rect {
    Rect {
        min: Point { x: left, y: top },
        max: Point {
            x: right,
            y: bottom,
        },
    }
}
const ICON: Point = Point { x: 20., y: 22. };
const TITLE_X: f64 = 46.;
const FIELD: Rect = area(20., 58., 400., 122.);
const HINT: Rect = area(20., 132., 400., 184.);
const CANCEL: Rect = area(216., 196., 304., 228.);
const SAVE: Rect = area(312., 196., 400., 228.);

#[derive(Clone, Copy, Default, PartialEq)]
pub(super) enum ShortcutTarget {
    #[default]
    Capture,
    Instant,
    Arrow,
    Rectangle,
    Ellipse,
    Board,
}
impl ShortcutTarget {
    pub fn shape_index(self) -> Option<usize> {
        match self {
            Self::Capture | Self::Instant | Self::Board => None,
            Self::Arrow => Some(0),
            Self::Rectangle => Some(1),
            Self::Ellipse => Some(2),
        }
    }
    /// The notification menu row that opens this shortcut; the window borrows its icon.
    fn menu_id(self) -> u32 {
        match self {
            Self::Capture => 2,
            Self::Instant => 6,
            Self::Arrow => 110,
            Self::Rectangle => 111,
            Self::Ellipse => 112,
            Self::Board => 13,
        }
    }
    fn title(self) -> &'static str {
        match self {
            Self::Capture => "Capture shortcut",
            Self::Instant => "Instant screenshot shortcut",
            Self::Arrow => "Arrow shortcut",
            Self::Rectangle => "Rectangle shortcut",
            Self::Ellipse => "Ellipse shortcut",
            Self::Board => "Board shortcut",
        }
    }
}

#[derive(Default)]
pub(super) struct Setup {
    pub target: ShortcutTarget,
    pub pending: Option<(u32, u32)>,
    pub recording: bool,
    pub focus: usize,
    pub error: String,
    /// Modifiers held while recording, shown before their companion key arrives.
    pub held: u32,
    pub painter: Option<Renderer>,
}
impl Setup {
    /// Opened from the menu: that click already asked for a change, so it listens at once.
    pub fn change(target: ShortcutTarget, current: Option<(u32, u32)>) -> Self {
        Self {
            target,
            pending: current,
            recording: true,
            ..Default::default()
        }
    }
    /// First launch proposes Print Screen; Save confirms it, clicking the field changes it.
    pub fn first_run() -> Self {
        Self {
            pending: Some((0, crate::print_screen::KEY)),
            focus: 2,
            ..Default::default()
        }
    }
    fn hint(&self) -> &str {
        let any_key = matches!(
            self.target,
            ShortcutTarget::Capture | ShortcutTarget::Instant
        );
        if !self.error.is_empty() {
            &self.error
        } else if self.recording {
            if any_key {
                "Any key works, alone or with Ctrl, Shift, Alt or Windows."
            } else if self.target == ShortcutTarget::Board {
                "A key with Ctrl or Alt. Shift or Windows can also be added."
            } else {
                "A letter with Ctrl or Alt. Shift can also be added."
            }
        } else if any_key
            && self
                .pending
                .is_some_and(|(m, k)| crate::print_screen::is_bare(m, k))
        {
            "Print Screen will open Shotoshot instead of the Windows Snipping Tool. Click the key to choose another one."
        } else {
            "Click the keys to change them."
        }
    }
    pub fn record(&mut self, mods: u32, key: u32) {
        if [16, 17, 18, 91, 92, 160, 161, 162, 163, 164, 165].contains(&key) {
            return;
        }
        self.pending = Some((mods, key));
        self.recording = false;
        self.focus = 2;
        self.error.clear();
    }
}

fn modifier_labels(mods: u32) -> impl Iterator<Item = &'static str> {
    [(2, "Ctrl"), (4, "Shift"), (1, "Alt"), (8, "Win")]
        .into_iter()
        .filter(move |(mask, _)| mods & mask != 0)
        .map(|(_, label)| label)
}
/// Modifiers then the key: one entry per keycap.
fn key_parts(mods: u32, key: u32) -> Vec<String> {
    let mut parts: Vec<String> = modifier_labels(mods).map(String::from).collect();
    let name = match key {
        8 => "Backspace".into(),
        9 => "Tab".into(),
        13 => "Enter".into(),
        20 => "Caps Lock".into(),
        27 => "Esc".into(),
        32 => "Space".into(),
        33 => "Page Up".into(),
        34 => "Page Down".into(),
        35 => "End".into(),
        36 => "Home".into(),
        37 => "Left".into(),
        38 => "Up".into(),
        39 => "Right".into(),
        40 => "Down".into(),
        45 => "Insert".into(),
        46 => "Delete".into(),
        96..=105 => format!("Numpad {}", key - 96),
        106 => "Numpad *".into(),
        107 => "Numpad +".into(),
        109 => "Numpad -".into(),
        110 => "Numpad .".into(),
        111 => "Numpad /".into(),
        112..=135 => format!("F{}", key - 111),
        144 => "Num Lock".into(),
        145 => "Scroll Lock".into(),
        19 => "Pause".into(),
        3 => "Break".into(),
        44 => "Print Screen".into(),
        _ => unsafe {
            let scan = MapVirtualKeyW(key, MAPVK_VK_TO_VSC);
            let extended = matches!(key, 33..=40 | 45 | 46 | 111 | 144);
            let mut buffer = [0u16; 128];
            let len = GetKeyNameTextW(
                ((scan << 16) | (u32::from(extended) << 24)) as i32,
                &mut buffer,
            );
            if len > 0 {
                String::from_utf16_lossy(&buffer[..len as usize])
            } else {
                format!("Key {key:02X}")
            }
        },
    };
    parts.push(name);
    parts
}
pub(super) fn key_label(mods: u32, key: u32) -> String {
    key_parts(mods, key).join(" + ")
}

/// Explains a Print Screen that stays taken once Shotoshot has freed it from the Snipping Tool.
pub(super) fn print_screen_error(mods: u32, key: u32, error: &Error) -> Option<String> {
    (crate::print_screen::is_bare(mods, key) && crate::print_screen::held(error)).then(|| {
        "Print Screen is still in use, by another app or by Windows until you sign out and back in. Close that app or choose another key.".into()
    })
}

/// The action under a logical point: 0 the shortcut field, 1 Cancel, 2 Save.
pub(super) fn hit(x: f64, y: f64) -> Option<usize> {
    [FIELD, CANCEL, SAVE]
        .iter()
        .position(|zone| zone.contains(Point { x, y }))
}

impl App {
    /// Gives Print Screen back to the Snipping Tool once no Shotoshot shortcut uses it.
    pub(super) fn restore_print_screen_if_unused(&self) {
        let s = &self.settings;
        if self.persist_preferences
            && !crate::print_screen::is_bare(s.modifiers, s.key)
            && !crate::print_screen::is_bare(s.instant_shortcut.modifiers, s.instant_shortcut.key)
        {
            // Best effort: the shortcut is already saved; the uninstaller retries the restore.
            let _ = crate::print_screen::restore();
        }
    }
    pub(super) fn setup_activate(&mut self, hwnd: HWND, action: usize) -> Result<()> {
        self.setup_ui.focus = action;
        match action {
            0 => {
                self.setup_ui.recording = true;
                self.setup_ui.error.clear();
            }
            1 => self.close_setup(hwnd)?,
            2 => {
                let Some((mods, key)) = self.setup_ui.pending else {
                    return Ok(());
                };
                if let Some(index) = self.setup_ui.target.shape_index() {
                    if self.save_shape_shortcut(hwnd, index, mods, key)? {
                        self.close_setup(hwnd)?;
                        self.notify(Notice::Preferences);
                    }
                    invalidate(hwnd);
                    return Ok(());
                }
                if self.setup_ui.target == ShortcutTarget::Instant {
                    self.save_instant_shortcut(hwnd, mods, key)?;
                    invalidate(hwnd);
                    return Ok(());
                }
                if self.setup_ui.target == ShortcutTarget::Board {
                    return self
                        .save_board_shortcut(hwnd, mods, key)
                        .map(|_| invalidate(hwnd));
                }
                if self.settings.instant_shortcut
                    == (crate::settings::Shortcut {
                        modifiers: mods,
                        key,
                    })
                {
                    self.setup_ui.error =
                        "This shortcut is already used for instant screenshots.".into();
                    invalidate(hwnd);
                    return Ok(());
                }
                if self
                    .settings
                    .presentation
                    .shape_shortcuts
                    .iter()
                    .any(|s| s.modifiers == mods && s.key == key)
                    || (mods == 3 && key == 27)
                {
                    self.setup_ui.error = "This shortcut is already used by presentation mode. Choose another shortcut.".into();
                    invalidate(hwnd);
                    return Ok(());
                }
                if self.settings.board_shortcut
                    == (crate::settings::Shortcut {
                        modifiers: mods,
                        key,
                    })
                {
                    self.setup_ui.error = "This shortcut is already used by the board.".into();
                    invalidate(hwnd);
                    return Ok(());
                }
                if !(self.hotkey.active()
                    && self.settings.key == key
                    && self.settings.modifiers == mods)
                {
                    let mut next = self.settings.clone();
                    next.key = key;
                    next.modifiers = mods;
                    let free = crate::print_screen::windows_option(self.persist_preferences);
                    if let Err(error) = self.hotkey.replace(free, mods, key, || {
                        if self.persist_preferences {
                            next.save().map_err(|e| Error::new(E_FAIL, e))
                        } else {
                            Ok(())
                        }
                    }) {
                        self.setup_ui.error = print_screen_error(mods, key, &error)
                            .unwrap_or_else(|| {
                                format!(
                                    "Unable to register this shortcut ({}). It may be reserved or already in use.",
                                    error.code().0
                                )
                            });
                        invalidate(hwnd);
                        return Ok(());
                    }
                    self.settings = next;
                    self.restore_print_screen_if_unused();
                }
                self.close_setup(hwnd)?;
                self.notify(Notice::Preferences);
            }
            _ => {}
        }
        invalidate(hwnd);
        Ok(())
    }
    /// Saves the shortcut that opens a board session; it needs Ctrl or Alt.
    pub(super) fn save_board_shortcut(&mut self, hwnd: HWND, mods: u32, key: u32) -> Result<()> {
        let shortcut = crate::settings::Shortcut {
            modifiers: mods,
            key,
        };
        if let Err(error) = Settings::validate_board_shortcut(&shortcut) {
            self.setup_ui.error = error;
            return Ok(());
        }
        if (self.settings.modifiers == mods && self.settings.key == key)
            || self.settings.instant_shortcut == shortcut
            || self
                .settings
                .presentation
                .shape_shortcuts
                .contains(&shortcut)
        {
            self.setup_ui.error = "This shortcut is already used by Shotoshot.".into();
            return Ok(());
        }
        if !(self.board_hotkey.active() && self.settings.board_shortcut == shortcut) {
            let mut next = self.settings.clone();
            next.board_shortcut = shortcut;
            // Ctrl or Alt is required: never a bare Print Screen to take back from Windows.
            if let Err(error) = self.board_hotkey.replace(None, mods, key, || {
                if self.persist_preferences {
                    next.save().map_err(|e| Error::new(E_FAIL, e))
                } else {
                    Ok(())
                }
            }) {
                self.setup_ui.error = format!(
                    "Shortcut unavailable ({}). Choose another key.",
                    error.code().0
                );
                return Ok(());
            }
            self.settings = next;
        }
        self.close_setup(hwnd)?;
        self.notify(Notice::Preferences);
        Ok(())
    }
}

const BACKGROUND: [u8; 3] = [0x20, 0x20, 0x20];
/// The shortcut field and the Cancel button.
const SURFACE: [u8; 3] = [0x2b, 0x2b, 0x2b];
const KEYCAP: [u8; 3] = [0x3a, 0x3a, 0x3a];
const KEYCAP_EDGE: [u8; 3] = [0x56, 0x56, 0x56];
const DISABLED: [u8; 3] = [0x33, 0x33, 0x33];
const ERROR: [u8; 3] = [0xf8, 0x51, 0x49];
/// Toolbar glyphs sit on a 20 px grid; the title shows its icon at 16 px.
const ICON_SCALE: f64 = 0.8;
const TITLE_SIZE: f64 = 16.;
const TEXT_SIZE: f64 = 13.;
const HINT_SIZE: f64 = 12.;
const CAP_HEIGHT: f64 = 28.;
const CAP_MARGIN: f64 = 10.;
const GAP: f64 = 6.;
/// Room kept between the field's edge and its keys.
const FIELD_MARGIN: f64 = 16.;
const RADIUS: f64 = 5.;

/// What the shortcut field shows, left to right.
enum Piece {
    Cap(String),
    Plus,
    Prompt(&'static str),
}
fn caps(labels: impl Iterator<Item = String>) -> Vec<Piece> {
    let mut pieces = Vec::new();
    for label in labels {
        if !pieces.is_empty() {
            pieces.push(Piece::Plus);
        }
        pieces.push(Piece::Cap(label));
    }
    pieces
}
fn pieces(ui: &Setup) -> Vec<Piece> {
    if ui.recording {
        let mut held = caps(modifier_labels(ui.held).map(String::from));
        if held.is_empty() {
            return vec![Piece::Prompt("Press your shortcut…")];
        }
        held.extend([Piece::Plus, Piece::Prompt("…")]);
        held
    } else if let Some((mods, key)) = ui.pending {
        caps(key_parts(mods, key).into_iter())
    } else {
        vec![Piece::Prompt("Choose a shortcut")]
    }
}

/// One line of text centred in `r`.
fn centered<C: Canvas>(
    canvas: &mut C,
    text: &str,
    r: Rect,
    size: f64,
    rgb: [u8; 3],
) -> std::result::Result<(), C::Error> {
    let extent = canvas.measure_text(text, size)?;
    let min = r.center() - extent * 0.5;
    canvas.text(
        text,
        Rect {
            min,
            // Measuring and drawing round differently: never wrap or clip the last glyph.
            max: min + extent + Point { x: size, y: 0. },
        },
        size,
        Ink::solid(rgb),
    )
}
fn button<C: Canvas>(
    canvas: &mut C,
    r: Rect,
    scale: f64,
    label: &str,
    [fill, edge, ink]: [[u8; 3]; 3],
) -> std::result::Result<(), C::Error> {
    canvas.fill_round_rect(r, RADIUS * scale, Ink::solid(fill))?;
    canvas.stroke_round_rect(
        r.expanded(-0.5 * scale),
        RADIUS * scale,
        Ink::solid(edge),
        scale,
    )?;
    centered(canvas, label, r, TEXT_SIZE * scale, ink)
}

/// The whole window, at `scale` device pixels per logical pixel.
fn paint<C: Canvas>(canvas: &mut C, ui: &Setup, scale: f64) -> std::result::Result<(), C::Error> {
    let u = |value: f64| value * scale;
    let px = |r: Rect| Rect {
        min: r.min * scale,
        max: r.max * scale,
    };
    canvas.fill_rect(
        px(area(0., 0., WIDTH as f64, HEIGHT as f64)),
        Ink::solid(BACKGROUND),
    )?;
    tray_menu::draw_icon(
        canvas,
        tray_menu::icon(ui.target.menu_id()),
        ICON * scale,
        u(ICON_SCALE),
        SECONDARY,
    )?;
    let title = canvas.measure_text(ui.target.title(), u(TITLE_SIZE))?;
    let title_top = u(ICON.y + 10. * ICON_SCALE) - title.y / 2.;
    canvas.text(
        ui.target.title(),
        Rect {
            min: Point {
                x: u(TITLE_X),
                y: title_top,
            },
            max: Point {
                x: u(WIDTH as f64),
                y: title_top + title.y,
            },
        },
        u(TITLE_SIZE),
        Ink::solid(TEXT),
    )?;

    let field = px(FIELD);
    canvas.fill_round_rect(field, u(RADIUS + 1.), Ink::solid(SURFACE))?;
    canvas.stroke_round_rect(
        field.expanded(-u(0.5)),
        u(RADIUS + 1.),
        Ink::solid(if ui.recording || ui.focus == 0 {
            ACCENT
        } else {
            RULE
        }),
        u(1.),
    )?;
    let mut slots = Vec::new();
    for piece in pieces(ui) {
        let width = match &piece {
            Piece::Cap(label) => (canvas.measure_text(label, u(TEXT_SIZE))?.x + u(2. * CAP_MARGIN))
                .max(u(CAP_HEIGHT)),
            Piece::Plus => canvas.measure_text("+", u(TEXT_SIZE))?.x,
            Piece::Prompt(text) => canvas.measure_text(text, u(TEXT_SIZE))?.x,
        };
        slots.push((piece, width.ceil()));
    }
    let span = |slots: &[(Piece, f64)]| {
        slots.iter().map(|(_, width)| width).sum::<f64>()
            + u(GAP) * slots.len().saturating_sub(1) as f64
    };
    if span(&slots) > field.size().x - u(2. * FIELD_MARGIN) {
        // A shortcut too long for the field keeps its keys and gives up the separators.
        slots.retain(|(piece, _)| !matches!(piece, Piece::Plus));
    }
    let total = span(&slots);
    let middle = field.center();
    let mut left = (middle.x - total / 2.).round();
    for (piece, width) in slots {
        let slot = Rect {
            min: Point {
                x: left,
                y: middle.y - u(CAP_HEIGHT / 2.),
            },
            max: Point {
                x: left + width,
                y: middle.y + u(CAP_HEIGHT / 2.),
            },
        };
        match piece {
            Piece::Cap(label) => button(canvas, slot, scale, &label, [KEYCAP, KEYCAP_EDGE, TEXT])?,
            Piece::Plus => centered(canvas, "+", slot, u(TEXT_SIZE), MUTED)?,
            Piece::Prompt(text) => centered(canvas, text, slot, u(TEXT_SIZE), SECONDARY)?,
        }
        left += width + u(GAP);
    }

    canvas.text(
        ui.hint(),
        px(HINT),
        u(HINT_SIZE),
        Ink::solid(if ui.error.is_empty() { MUTED } else { ERROR }),
    )?;
    button(
        canvas,
        px(CANCEL),
        scale,
        "Cancel",
        [SURFACE, if ui.focus == 1 { ACCENT } else { RULE }, TEXT],
    )?;
    let fill = if ui.pending.is_some() && !ui.recording {
        ACCENT
    } else {
        DISABLED
    };
    button(
        canvas,
        px(SAVE),
        scale,
        "Save",
        [
            fill,
            if ui.focus == 2 { TEXT } else { fill },
            if fill == ACCENT { BACKGROUND } else { MUTED },
        ],
    )
}

impl Setup {
    /// Paints the window over `bounds` of `dc`. The first paint creates the painter and
    /// closing the window drops it: the resident keeps no renderer at rest.
    pub fn draw(&mut self, dc: HDC, bounds: RECT) -> Result<()> {
        if bounds.right <= bounds.left || bounds.bottom <= bounds.top {
            return Ok(());
        }
        // Software like the menu's: a Direct3D device would outlive the window in the resident.
        let renderer = match self.painter.take() {
            Some(renderer) => renderer,
            None => Renderer::software()?,
        };
        let scale = (bounds.right - bounds.left) as f64 / WIDTH as f64;
        let painted = unsafe {
            renderer.dc.BindDC(dc, &bounds).and_then(|()| {
                renderer.dc.BeginDraw();
                renderer
                    .dc
                    .SetTransform(&windows_numerics::Matrix3x2::identity());
                let painted = D2dCanvas::new(&renderer, &renderer.dc)
                    .and_then(|mut canvas| paint(&mut canvas, self, scale));
                let ended = renderer.dc.EndDraw(None, None);
                painted.and(ended)
            })
        };
        // A lost render target is rebuilt by the next paint.
        if painted.is_ok() {
            self.painter = Some(renderer);
        }
        painted
    }
}

fn render(ui: &mut Setup, scale: i32) -> Result<Image> {
    let (width, height) = (WIDTH * scale, HEIGHT * scale);
    paint_image(width as u32, height as u32, |dc| {
        ui.draw(
            dc,
            RECT {
                right: width,
                bottom: height,
                ..Default::default()
            },
        )
    })
}
pub(super) fn preview() -> Result<()> {
    notice::preview()?;
    for (name, mut ui) in [
        (
            "shotoshot-welcome.png",
            Setup::change(ShortcutTarget::Capture, None),
        ),
        ("shotoshot-print-screen.png", Setup::first_run()),
        (
            "shotoshot-shortcut.png",
            Setup {
                target: ShortcutTarget::Arrow,
                pending: Some((6, 0x46)),
                focus: 2,
                ..Default::default()
            },
        ),
    ] {
        save_png(
            &render(&mut ui, 1)?,
            &std::path::Path::new("target").join(name),
        )?;
    }
    tray_menu::preview()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pixels of the logical area `r` whose RGB colour passes `test`.
    fn count(image: &Image, r: Rect, scale: i32, test: impl Fn([u8; 3]) -> bool) -> usize {
        let scale = scale as f64;
        let columns = (r.min.x * scale) as usize..(r.max.x * scale) as usize;
        let rows = (r.min.y * scale) as usize..(r.max.y * scale) as usize;
        image
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter(|(index, pixel)| {
                columns.contains(&(index % image.width as usize))
                    && rows.contains(&(index / image.width as usize))
                    && test([pixel[2], pixel[1], pixel[0]])
            })
            .count()
    }
    /// Keycaps crossed by a scan line just under their top edge, where no glyph reaches.
    fn keycaps(image: &Image, scale: i32) -> usize {
        let row = ((FIELD.center().y - CAP_HEIGHT / 2. + 3.) * scale as f64) as usize;
        let line = &image.pixels[row * image.width as usize * 4..][..image.width as usize * 4];
        let cap: Vec<bool> = line
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| [pixel[2], pixel[1], pixel[0]] == KEYCAP)
            .collect();
        cap.windows(2).filter(|pair| !pair[0] && pair[1]).count()
    }
    fn shown(pending: (u32, u32)) -> Setup {
        Setup {
            pending: Some(pending),
            focus: 2,
            ..Default::default()
        }
    }

    #[test]
    fn a_shortcut_shows_one_keycap_per_key() {
        for (pending, expected) in [((0, 19), 1), ((2, 0x46), 2), ((6, 19), 3), ((15, 0x2C), 5)] {
            let image = render(&mut shown(pending), 1).unwrap();
            assert_eq!(keycaps(&image, 1), expected, "{pending:?}");
        }
    }

    #[test]
    fn the_longest_shortcut_stays_inside_its_field() {
        let image = render(&mut shown((15, 0x2C)), 1).unwrap();
        assert_eq!(keycaps(&image, 1), 5, "every key stays, separators go");
        // On the rows the keycaps occupy, nothing is drawn in the field's margins.
        let (top, bottom) = (
            FIELD.center().y - CAP_HEIGHT / 2.,
            FIELD.center().y + CAP_HEIGHT / 2.,
        );
        for (from, to) in [
            (FIELD.min.x + 2., FIELD.min.x + FIELD_MARGIN),
            (FIELD.max.x - FIELD_MARGIN, FIELD.max.x - 2.),
        ] {
            let margin = area(from, top, to, bottom);
            assert_eq!(count(&image, margin, 1, |rgb| rgb != SURFACE), 0);
        }
    }

    #[test]
    fn opening_from_the_menu_listens_at_once_and_frames_the_field() {
        let mut ui = Setup::change(ShortcutTarget::Instant, Some((0, 145)));
        assert!(ui.recording);
        assert_eq!(
            ui.pending,
            Some((0, 145)),
            "Cancel keeps the current shortcut"
        );
        let accent = |rgb| rgb == ACCENT;
        let listening = render(&mut ui, 1).unwrap();
        assert!(count(&listening, FIELD, 1, accent) > 300);
        assert_eq!(keycaps(&listening, 1), 0);
        assert_eq!(
            count(&listening, SAVE, 1, accent),
            0,
            "nothing to save while listening"
        );
        ui.held = 6;
        assert_eq!(
            keycaps(&render(&mut ui, 1).unwrap(), 1),
            2,
            "held modifiers show before the key"
        );
        ui.record(6, 19);
        let recorded = render(&mut ui, 1).unwrap();
        assert_eq!(count(&recorded, FIELD, 1, accent), 0);
        assert_eq!(keycaps(&recorded, 1), 3);
        assert!(count(&recorded, SAVE, 1, accent) > 1500);
    }

    #[test]
    fn clicks_land_on_what_is_painted() {
        for (zone, action) in [(FIELD, 0), (CANCEL, 1), (SAVE, 2)] {
            let (center, inside) = (zone.center(), zone.expanded(-1.));
            assert_eq!(hit(center.x, center.y), Some(action));
            assert_eq!(hit(inside.min.x, inside.min.y), Some(action));
            assert_eq!(hit(inside.max.x, inside.max.y), Some(action));
        }
        assert_eq!(hit(ICON.x, ICON.y), None);
        assert_eq!(hit(CANCEL.max.x + 4., CANCEL.center().y), None);
        let image = render(&mut shown((0, 19)), 1).unwrap();
        // Just inside each zone's top edge, above any text: the control covers the window.
        for zone in [FIELD, CANCEL, SAVE] {
            let strip = area(
                zone.min.x + 4.,
                zone.min.y + 2.,
                zone.max.x - 4.,
                zone.min.y + 5.,
            );
            assert_eq!(count(&image, strip, 1, |rgb| rgb == BACKGROUND), 0);
        }
    }

    #[test]
    fn each_shortcut_shows_the_icon_of_its_menu_row() {
        let icon = Rect {
            min: ICON,
            max: ICON + Point { x: 16., y: 16. },
        };
        let drawn = |target| {
            let image = render(
                &mut Setup {
                    target,
                    ..Default::default()
                },
                1,
            )
            .unwrap();
            count(&image, icon, 1, |rgb| rgb != BACKGROUND)
        };
        let inks = [
            ShortcutTarget::Capture,
            ShortcutTarget::Instant,
            ShortcutTarget::Arrow,
            ShortcutTarget::Rectangle,
            ShortcutTarget::Ellipse,
            ShortcutTarget::Board,
        ]
        .map(drawn);
        assert!(inks.iter().all(|&ink| ink > 20), "{inks:?}");
        assert_ne!(
            inks[3], inks[4],
            "rectangle and ellipse are different glyphs"
        );
    }

    #[test]
    fn the_board_shortcut_asks_for_ctrl_or_alt() {
        let mut ui = Setup::change(ShortcutTarget::Board, Some((1, 0x42)));
        assert!(ui.hint().contains("Ctrl or Alt"), "{}", ui.hint());
        ui.record(0, crate::print_screen::KEY);
        assert!(
            !ui.hint().contains("Snipping Tool"),
            "the board never takes Print Screen from Windows"
        );
    }

    #[test]
    fn an_error_replaces_the_hint_in_red() {
        let red = |rgb: [u8; 3]| rgb[0] > 150 && rgb[0] as i32 - rgb[1] as i32 > 60;
        let mut ui = shown((1, 0x52));
        assert_eq!(count(&render(&mut ui, 1).unwrap(), HINT, 1, red), 0);
        ui.error = print_screen_error(
            0,
            crate::print_screen::KEY,
            &Error::from_hresult(HRESULT::from_win32(ERROR_HOTKEY_ALREADY_REGISTERED.0)),
        )
        .unwrap();
        let image = render(&mut ui, 1).unwrap();
        assert!(count(&image, HINT, 1, red) > 100);
        // The longest message wraps inside the hint area instead of reaching the buttons.
        let below = area(HINT.min.x, HINT.max.y, HINT.max.x, CANCEL.min.y);
        assert_eq!(count(&image, below, 1, |rgb| rgb != BACKGROUND), 0);
    }

    #[test]
    fn the_window_scales_with_the_monitor() {
        let (normal, doubled) = (
            render(&mut shown((6, 19)), 1).unwrap(),
            render(&mut shown((6, 19)), 2).unwrap(),
        );
        assert_eq!(
            (doubled.width, doubled.height),
            (2 * WIDTH as u32, 2 * HEIGHT as u32)
        );
        assert_eq!(keycaps(&doubled, 2), 3);
        let accent = |rgb| rgb == ACCENT;
        let ratio =
            count(&doubled, SAVE, 2, accent) as f64 / count(&normal, SAVE, 1, accent) as f64;
        assert!((3.5..=5.).contains(&ratio), "{ratio}");
    }

    #[test]
    fn only_painting_creates_the_painter() {
        let mut ui = Setup::change(ShortcutTarget::Capture, None);
        assert!(ui.painter.is_none());
        render(&mut ui, 1).unwrap();
        assert!(ui.painter.is_some(), "kept between paints of one opening");
        assert!(Setup::first_run().painter.is_none());
    }
}
