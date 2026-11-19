//! ROLE: The shortcut window: what it records, how it is painted and where it is clicked.
//! OWNER: App holds one `Setup` per opening; its painter lives only while the window is shown.
//! INVARIANTS: Layout is in 96-DPI logical pixels, scaled once at paint time. Painting and
//! clicks read the same `SetupLayout`; English keeps the sizes the window had before translation.
use super::*;
use crate::render::{D2dCanvas, Renderer};
use shotoshot_text::{Key, Language, Text, tr_in};

use shotoshot_ui::shortcut_window::{self as window, Piece};
// Names the tests and the rest of the app use; the definitions are shared with Linux.
#[cfg(test)]
use shotoshot_core::Rect;
#[cfg(test)]
use window::{
    ACCENT, BACKGROUND, BUTTON_PADDING, CAP_HEIGHT, FIELD_MARGIN, HINT_SIZE, ICON, KEYCAP, MARGIN,
    SURFACE, TEXT_SIZE, TITLE_SIZE, TITLE_X, Widths, area,
};
/// The widest content of the field in `language`, as `renderer` measures it.
#[cfg(test)]
fn keys_width(renderer: &Renderer, language: Language) -> Result<f64> {
    window::keys_width(language, &windows_name, &mut |text, size| {
        Ok(renderer.measure_text_in(text, size as f32, language)?.x)
    })
}
pub(super) use window::{Layout as SetupLayout, modifier_labels_in};

/// The window sized for the texts of `language`, as `renderer` measures them.
pub(super) fn layout(renderer: &Renderer, language: Language) -> Result<SetupLayout> {
    let titles: Vec<&str> = TARGETS
        .iter()
        .map(|target| tr_in(language, target.title()))
        .collect();
    SetupLayout::measured(
        language,
        &titles,
        &hints_in(language),
        &windows_name,
        &mut |text, size| Ok(renderer.measure_text_in(text, size as f32, language)?.x),
        &mut |text, size, wrap| {
            Ok(renderer
                .measure_wrapped_text_in(text, size as f32, wrap as f32, language)?
                .y)
        },
    )
}

#[derive(Clone, Copy, Default, PartialEq)]
pub(super) enum ShortcutTarget {
    #[default]
    Capture,
    Instant,
    Arrow,
    Rectangle,
    Ellipse,
    Board,
    /// The pointer window's shortcut that shows a click.
    ShowClick,
    /// The pointer window's shortcut that holds the spotlight.
    Spotlight,
    /// The pointer window's shortcut that shows or holds the loupe.
    Loupe,
}
/// Every window the menu opens: the layout fits the title of each.
const TARGETS: [ShortcutTarget; 9] = [
    ShortcutTarget::Capture,
    ShortcutTarget::Instant,
    ShortcutTarget::Arrow,
    ShortcutTarget::Rectangle,
    ShortcutTarget::Ellipse,
    ShortcutTarget::Board,
    ShortcutTarget::ShowClick,
    ShortcutTarget::Spotlight,
    ShortcutTarget::Loupe,
];
impl ShortcutTarget {
    pub fn shape_index(self) -> Option<usize> {
        match self {
            Self::Capture
            | Self::Instant
            | Self::Board
            | Self::ShowClick
            | Self::Spotlight
            | Self::Loupe => None,
            Self::Arrow => Some(0),
            Self::Rectangle => Some(1),
            Self::Ellipse => Some(2),
        }
    }
    /// The notification menu row that opens this shortcut; the window borrows its icon.
    /// The icon of the action, as the menu and the toolbar draw it.
    fn icon(self) -> tray_menu::Icon {
        use shotoshot_core::Tool;
        use tray_menu::Icon;
        match self {
            Self::Capture => Icon::Capture,
            Self::Instant => Icon::Instant,
            Self::Arrow => Icon::Toolbar(Action::Tool(Tool::Arrow)),
            Self::Rectangle => Icon::Toolbar(Action::Tool(Tool::Rectangle)),
            Self::Ellipse => Icon::Toolbar(Action::Tool(Tool::Ellipse)),
            Self::Board => Icon::BoardEdit,
            Self::ShowClick => Icon::Click,
            Self::Spotlight => Icon::Spotlight,
            Self::Loupe => Icon::Loupe,
        }
    }
    fn title(self) -> Key {
        match self {
            Self::Capture => Key::SetupTitleCapture,
            Self::Instant => Key::SetupTitleInstant,
            Self::Arrow => Key::SetupTitleArrow,
            Self::Rectangle => Key::SetupTitleRectangle,
            Self::Ellipse => Key::SetupTitleEllipse,
            Self::Board => Key::SetupTitleBoard,
            Self::ShowClick => Key::SetupTitleShowClick,
            Self::Spotlight => Key::SetupTitleSpotlight,
            Self::Loupe => Key::SetupTitleLoupe,
        }
    }
    /// The pointer shortcut this window records: 0 shows a click, 1 holds the spotlight, 2
    /// shows the loupe.
    pub fn pointer_index(self) -> Option<usize> {
        match self {
            Self::ShowClick => Some(0),
            Self::Spotlight => Some(1),
            Self::Loupe => Some(2),
            _ => None,
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
    /// The language of this opening: its texts and the layout that fits them.
    pub language: Language,
    pub layout: SetupLayout,
    pub painter: Option<Renderer>,
    /// Changed in its row of Settings, with no window of its own.
    pub inline: bool,
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
    /// First launch proposes Print Screen; Save confirms it, any other key replaces it.
    pub fn first_run() -> Self {
        Self::confirm((0, crate::print_screen::KEY))
    }
    /// Shown at the end of every install: the capture shortcut as it is, Save keeps it and
    /// pressing another key replaces it (Mickael, 2026-11-04).
    pub fn confirm(current: (u32, u32)) -> Self {
        Self {
            pending: Some(current),
            focus: 2,
            ..Default::default()
        }
    }
    /// Sizes the window for `language` before it is shown. The renderer that measured becomes
    /// the painter, dropped with it when the window closes.
    pub fn fit(&mut self, language: Language) {
        let renderer = match self.painter.take() {
            Some(renderer) => Ok(renderer),
            None => Renderer::software(),
        };
        let measured = renderer.and_then(|renderer| Ok((layout(&renderer, language)?, renderer)));
        self.place(language, measured);
    }
    /// Takes the layout `measured` for `language`. A renderer that could not measure leaves
    /// the English layout: the window opens all the same, and painting makes its own renderer.
    fn place(&mut self, language: Language, measured: Result<(SetupLayout, Renderer)>) {
        self.language = language;
        match measured {
            Ok((layout, renderer)) => {
                self.layout = layout;
                self.painter = Some(renderer);
            }
            Err(_) => self.layout = SetupLayout::default(),
        }
    }
    /// The text of `key` in the language of this opening.
    pub fn text(&self, key: Key) -> String {
        tr_in(self.language, key).into()
    }
    fn hint(&self) -> &str {
        if self.error.is_empty() {
            tr_in(self.language, self.hint_key())
        } else {
            &self.error
        }
    }
    fn hint_key(&self) -> Key {
        let any_key = matches!(
            self.target,
            ShortcutTarget::Capture | ShortcutTarget::Instant
        );
        if self.recording {
            if any_key {
                Key::SetupAnyKeyHint
            } else if matches!(
                self.target,
                ShortcutTarget::Board
                    | ShortcutTarget::ShowClick
                    | ShortcutTarget::Spotlight
                    | ShortcutTarget::Loupe
            ) {
                Key::SetupBoardHint
            } else {
                Key::SetupShapeHint
            }
        } else if any_key
            && self
                .pending
                .is_some_and(|(m, k)| crate::print_screen::is_bare(m, k))
        {
            Key::SetupPrintScreenHint
        } else {
            Key::SetupChangeHint
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

/// Every text the hint area can show in `language`, errors included, each with its longest
/// value: the layout makes room for the tallest.
fn hints_in(language: Language) -> Vec<String> {
    use shotoshot_core::settings::{BOARD_SHORTCUT_REFUSALS, SHAPE_SHORTCUT_REFUSALS};
    // A board shortcut is refused alone, a shape shortcut inside `setup.invalid`
    // (`App::save_shape_shortcut`).
    let mut texts: Vec<String> = [
        Key::SetupAnyKeyHint,
        Key::SetupBoardHint,
        Key::SetupShapeHint,
        Key::SetupPrintScreenHint,
        Key::SetupChangeHint,
        Key::SetupUsedForInstant,
        Key::SetupUsedByPresentation,
        Key::SetupUsedByBoard,
        Key::SetupUsedByShotoshot,
        Key::SetupPrintScreenHeld,
    ]
    .into_iter()
    .chain(BOARD_SHORTCUT_REFUSALS)
    .map(|key| tr_in(language, key).to_string())
    .collect();
    // The widest code `{code}` can print.
    let code = i32::MIN.to_string();
    for key in [Key::SetupRegisterFailed, Key::SetupUnavailable] {
        texts.push(Text::with_in(language, key, &[("code", &code)]).to_string());
    }
    for detail in SHAPE_SHORTCUT_REFUSALS
        .into_iter()
        .chain([Key::SetupUsedForCapture, Key::SetupUsedByBoard])
    {
        let detail = tr_in(language, detail);
        texts.push(Text::with_in(language, Key::SetupInvalid, &[("detail", detail)]).to_string());
    }
    texts
}

/// Modifiers then the key: one entry per keycap.
fn key_parts_in(mods: u32, key: u32, language: Language) -> Vec<String> {
    window::key_parts_in(mods, key, language, &windows_name)
}
/// The name Windows gives `key` on the current keyboard layout.
fn windows_name(key: u32) -> Option<String> {
    unsafe {
        let scan = MapVirtualKeyW(key, MAPVK_VK_TO_VSC);
        let extended = matches!(key, 33..=40 | 45 | 46 | 111 | 144);
        let mut buffer = [0u16; 128];
        let len = GetKeyNameTextW(
            ((scan << 16) | (u32::from(extended) << 24)) as i32,
            &mut buffer,
        );
        (len > 0).then(|| String::from_utf16_lossy(&buffer[..len as usize]))
    }
}
/// The combination as one line, in `language`.
pub(super) fn key_label_in(mods: u32, key: u32, language: Language) -> String {
    key_parts_in(mods, key, language).join(" + ")
}

/// Explains a Print Screen that stays taken once Shotoshot has freed it from the Snipping Tool.
pub(super) fn print_screen_error(
    mods: u32,
    key: u32,
    error: &Error,
    language: Language,
) -> Option<String> {
    (crate::print_screen::is_bare(mods, key) && crate::print_screen::held(error))
        .then(|| tr_in(language, Key::SetupPrintScreenHeld).into())
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
                if let Some(index) = self.setup_ui.target.pointer_index() {
                    self.save_pointer_shortcut(hwnd, index, mods, key)?;
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
                    self.setup_ui.error = self.setup_ui.text(Key::SetupUsedForInstant);
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
                    self.setup_ui.error = self.setup_ui.text(Key::SetupUsedByPresentation);
                    invalidate(hwnd);
                    return Ok(());
                }
                if self.settings.board_shortcut
                    == (crate::settings::Shortcut {
                        modifiers: mods,
                        key,
                    })
                {
                    self.setup_ui.error = self.setup_ui.text(Key::SetupUsedByBoard);
                    invalidate(hwnd);
                    return Ok(());
                }
                if !(self.hotkey.active()
                    && self.settings.key == key
                    && self.settings.modifiers == mods)
                    && let Err(error) = self.take_capture_shortcut(mods, key)
                {
                    let language = self.setup_ui.language;
                    self.setup_ui.error = print_screen_error(mods, key, &error, language)
                        .unwrap_or_else(|| {
                            let code = error.code().0.to_string();
                            Text::with_in(language, Key::SetupRegisterFailed, &[("code", &code)])
                                .to_string()
                        });
                    invalidate(hwnd);
                    return Ok(());
                }
                self.close_setup(hwnd)?;
                self.notify(Notice::Preferences);
            }
            _ => {}
        }
        invalidate(hwnd);
        Ok(())
    }
    /// Registers and saves the capture shortcut; on failure nothing changes.
    pub(super) fn take_capture_shortcut(&mut self, mods: u32, key: u32) -> Result<()> {
        let mut next = self.settings.clone();
        next.key = key;
        next.modifiers = mods;
        let free = crate::print_screen::windows_option(self.persist_preferences);
        self.hotkey.replace(free, mods, key, || {
            if self.persist_preferences {
                next.save().map_err(crate::settings::unsaved)
            } else {
                Ok(())
            }
        })?;
        self.settings = next;
        self.restore_print_screen_if_unused();
        Ok(())
    }
    /// Saves the shortcut that opens a board session; it needs Ctrl or Alt.
    pub(super) fn save_board_shortcut(&mut self, hwnd: HWND, mods: u32, key: u32) -> Result<()> {
        let shortcut = crate::settings::Shortcut {
            modifiers: mods,
            key,
        };
        if let Err(refusal) = Settings::validate_board_shortcut(&shortcut) {
            self.setup_ui.error = self.setup_ui.text(refusal);
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
            self.setup_ui.error = self.setup_ui.text(Key::SetupUsedByShotoshot);
            return Ok(());
        }
        if !(self.board_hotkey.active() && self.settings.board_shortcut == shortcut) {
            let mut next = self.settings.clone();
            next.board_shortcut = shortcut;
            // Ctrl or Alt is required: never a bare Print Screen to take back from Windows.
            if let Err(error) = self.board_hotkey.replace(None, mods, key, || {
                if self.persist_preferences {
                    next.save().map_err(crate::settings::unsaved)
                } else {
                    Ok(())
                }
            }) {
                let code = error.code().0.to_string();
                self.setup_ui.error = Text::with_in(
                    self.setup_ui.language,
                    Key::SetupUnavailable,
                    &[("code", &code)],
                )
                .to_string();
                return Ok(());
            }
            self.settings = next;
        }
        self.close_setup(hwnd)?;
        self.notify(Notice::Preferences);
        Ok(())
    }
}

/// What one frame of the window shows.
fn view(ui: &Setup) -> window::View<'_> {
    window::View {
        language: ui.language,
        layout: ui.layout,
        icon: ui.target.icon(),
        title: tr_in(ui.language, ui.target.title()),
        pieces: pieces(ui),
        hint: ui.hint(),
        error: !ui.error.is_empty(),
        field_framed: ui.recording || ui.focus == 0,
        focus: ui.focus,
        save_enabled: ui.pending.is_some() && !ui.recording,
    }
}
fn pieces(ui: &Setup) -> Vec<Piece> {
    window::pieces(
        ui.recording,
        ui.held,
        ui.pending,
        ui.language,
        &windows_name,
    )
}

impl Setup {
    /// Paints the window over `bounds` of `dc`. Opening or the first paint creates the painter
    /// and closing the window drops it: the resident keeps no renderer at rest.
    pub fn draw(&mut self, dc: HDC, bounds: RECT) -> Result<()> {
        if bounds.right <= bounds.left || bounds.bottom <= bounds.top {
            return Ok(());
        }
        // Software like the menu's: a Direct3D device would outlive the window in the resident.
        let renderer = match self.painter.take() {
            Some(renderer) => renderer,
            None => Renderer::software()?,
        };
        let scale = (bounds.right - bounds.left) as f64 / self.layout.size.x;
        let painted = unsafe {
            renderer.dc.BindDC(dc, &bounds).and_then(|()| {
                renderer.dc.BeginDraw();
                renderer
                    .dc
                    .SetTransform(&windows_numerics::Matrix3x2::identity());
                let painted = D2dCanvas::new(&renderer, &renderer.dc)
                    .and_then(|mut canvas| window::paint(&mut canvas, view(self), scale));
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
    let size = ui.layout.size;
    let (width, height) = (size.x as i32 * scale, size.y as i32 * scale);
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
    settings_window::preview()?;
    tray_menu::preview()
}

#[cfg(test)]
mod tests;
