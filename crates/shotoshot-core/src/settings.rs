//! ROLE: The settings file format shared by every platform: types, defaults, versioned
//! decoding and the merge-then-replace save.
//! OWNER: Each platform supplies the file path, a cross-process lock and the final replace.
//! INVARIANTS: Keys are Windows virtual-key codes on every platform; a damaged file is
//! reported and left untouched.
use shotoshot_text::{Key, LanguageChoice};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DisplayPosition {
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    #[default]
    BottomCenter,
    BottomRight,
}
impl DisplayPosition {
    pub const ALL: [Self; 6] = [
        Self::TopLeft,
        Self::TopCenter,
        Self::TopRight,
        Self::BottomLeft,
        Self::BottomCenter,
        Self::BottomRight,
    ];
    pub fn index(self) -> usize {
        match self {
            Self::TopLeft => 0,
            Self::TopCenter => 1,
            Self::TopRight => 2,
            Self::BottomLeft => 3,
            Self::BottomCenter => 4,
            Self::BottomRight => 5,
        }
    }
    fn decode(value: &str) -> std::result::Result<Self, String> {
        match value {
            "0" => Ok(Self::TopLeft),
            "1" => Ok(Self::TopCenter),
            "2" => Ok(Self::TopRight),
            "3" => Ok(Self::BottomLeft),
            "4" => Ok(Self::BottomCenter),
            "5" => Ok(Self::BottomRight),
            _ => Err("Invalid input display position".into()),
        }
    }
}
/// Image file format written by Save; the dialog reopens on the last one chosen.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SaveFormat {
    /// Lossless.
    #[default]
    Png,
    /// Always written at the highest quality: there is no quality setting.
    Jpeg,
}
impl SaveFormat {
    pub const ALL: [Self; 2] = [Self::Png, Self::Jpeg];
    pub fn index(self) -> usize {
        match self {
            Self::Png => 0,
            Self::Jpeg => 1,
        }
    }
    fn decode(value: &str) -> std::result::Result<Self, String> {
        match value {
            "0" => Ok(Self::Png),
            "1" => Ok(Self::Jpeg),
            _ => Err("Invalid save format".into()),
        }
    }
    /// Extension given to a new file.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }
    /// Format named by the extension of `path`, when it is one Save writes.
    pub fn of(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            "png" => Some(Self::Png),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            _ => None,
        }
    }
}
/// Every reason `PresentationSettings::validate_shape_shortcuts` can give.
pub const SHAPE_SHORTCUT_REFUSALS: [Key; 4] = [
    Key::SetupInvalidShape,
    Key::SetupShapeNeedsModifier,
    Key::SetupReserved,
    Key::SetupDuplicateShape,
];
/// Every reason `Settings::validate_board_shortcut` can give.
pub const BOARD_SHORTCUT_REFUSALS: [Key; 3] = [
    Key::SetupInvalidBoard,
    Key::SetupBoardNeedsModifier,
    Key::SetupReserved,
];
/// The technical detail of a refused shortcut found in a settings file.
fn refused(key: Key) -> String {
    format!("refused shortcut ({})", key.id())
}
/// Sizes of the presentation diagram, in percent of its natural size.
pub const INPUT_SCALES: [u32; 3] = [100, 150, 200];
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Shortcut {
    pub modifiers: u32,
    pub key: u32,
}
impl Shortcut {
    fn decode(text: &str) -> std::result::Result<Self, String> {
        let (modifiers, key) = text.split_once(',').ok_or("Invalid shape shortcut")?;
        let parse = |value: &str| {
            value
                .parse::<u32>()
                .map_err(|_| "Invalid shape shortcut".to_string())
        };
        Ok(Self {
            modifiers: parse(modifiers)?,
            key: parse(key)?,
        })
    }
}
/// Alt + the letter that selects the same shape in the capture editor (A, R, C).
const DEFAULT_SHAPE_SHORTCUTS: [Shortcut; 3] = [
    Shortcut {
        modifiers: 1,
        key: 0x41,
    },
    Shortcut {
        modifiers: 1,
        key: 0x52,
    },
    Shortcut {
        modifiers: 1,
        key: 0x43,
    },
];
/// Arrow default of formats 5 to 10: Alt+F.
const OLD_ARROW_SHORTCUT: Shortcut = Shortcut {
    modifiers: 1,
    key: 0x46,
};
#[derive(Clone, Debug, PartialEq)]
pub struct PresentationSettings {
    /// Arrow, rectangle, ellipse, in that order.
    pub shape_shortcuts: [Shortcut; 3],
    pub input_position: DisplayPosition,
    /// Normalized free position, each axis in 0..=10000.
    pub input_offset: Option<[u16; 2]>,
    /// One of `INPUT_SCALES`.
    pub input_scale: u32,
    pub duration_ms: u32,
    pub halo_color: [u8; 3],
    pub shape_color: [u8; 3],
    pub stroke_width: u32,
    pub arrow_size: u32,
    /// The pointer window's choices: halo shape, trail, clicks, spotlight.
    pub pointer: crate::pointer::PointerSettings,
    /// A hidden window OBS can capture, mirroring the presentation layers. Off by default:
    /// it costs a Direct3D device that only a streamer needs (2026-11-07).
    pub obs_mirror: bool,
}
impl Default for PresentationSettings {
    fn default() -> Self {
        Self {
            stroke_width: 5,
            arrow_size: 28,
            shape_shortcuts: DEFAULT_SHAPE_SHORTCUTS,
            input_position: DisplayPosition::default(),
            input_offset: None,
            input_scale: 100,
            duration_ms: 3000,
            halo_color: [255, 211, 64],
            shape_color: [237, 79, 92],
            pointer: crate::pointer::PointerSettings::default(),
            obs_mirror: false,
        }
    }
}
impl PresentationSettings {
    /// Why the shape shortcuts cannot be used, as the text the user reads: one of
    /// `SHAPE_SHORTCUT_REFUSALS`.
    pub fn validate_shape_shortcuts(&self) -> std::result::Result<(), Key> {
        for (index, shortcut) in self.shape_shortcuts.iter().enumerate() {
            if shortcut.modifiers > 15 || !(1..=254).contains(&shortcut.key) {
                return Err(Key::SetupInvalidShape);
            }
            if shortcut.modifiers & 3 == 0 {
                return Err(Key::SetupShapeNeedsModifier);
            }
            if shortcut.modifiers == 3 && shortcut.key == 27 {
                return Err(Key::SetupReserved);
            }
            if self.shape_shortcuts[..index].contains(shortcut) {
                return Err(Key::SetupDuplicateShape);
            }
        }
        Ok(())
    }
    fn decode(duration: &str, halo: &str, shape: &str) -> std::result::Result<Self, String> {
        let duration_ms = match duration {
            "1000" => 1000,
            "3000" => 3000,
            "5000" => 5000,
            _ => return Err("Invalid presentation duration".into()),
        };
        let color = |text: &str| -> std::result::Result<[u8; 3], String> {
            if text.len() != 6 || !text.bytes().all(|value| value.is_ascii_hexdigit()) {
                return Err("Invalid presentation color".into());
            }
            let value = u32::from_str_radix(text, 16)
                .map_err(|_| "Invalid presentation color".to_string())?;
            let [_, red, green, blue] = value.to_be_bytes();
            Ok([red, green, blue])
        };
        Ok(Self {
            stroke_width: 5,
            arrow_size: 28,
            shape_shortcuts: DEFAULT_SHAPE_SHORTCUTS,
            input_position: DisplayPosition::default(),
            input_offset: None,
            input_scale: 100,
            duration_ms,
            halo_color: color(halo)?,
            shape_color: color(shape)?,
            pointer: crate::pointer::PointerSettings::default(),
            obs_mirror: false,
        })
    }
}
/// The editor's sizes remembered between captures, at 100 %.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Brush {
    pub stroke_width: u32,
    pub eraser_size: u32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub instant_shortcut: Shortcut,
    pub start_with_windows: bool,
    pub capture_stroke_width: u32,
    /// The editor eraser's diameter at 100 %, in pixels.
    pub capture_eraser_size: u32,
    pub live_capture: bool,
    pub modifiers: u32,
    pub key: u32,
    // Legacy file-format slot, preserved for compatibility only.
    pub legacy_style: bool,
    pub folder: PathBuf,
    pub save_format: SaveFormat,
    pub presentation: PresentationSettings,
    pub last_custom_color: [u8; 3],
    /// Toggles the desktop board; Ctrl or Alt is required (see `validate_board_shortcut`).
    pub board_shortcut: Shortcut,
    /// Whether the board is currently shown in the wallpaper.
    pub board_visible: bool,
    /// The interface language: the system's, or one chosen in the menu.
    pub language: LanguageChoice,
    /// The user has resized a tool with the wheel once: the editor stops hinting at it.
    pub wheel_learned: bool,
    /// Captures open with the mini toolbar, unfolded by its button (issue #6). Off by default.
    pub mini_toolbar: bool,
    /// The mini toolbar was left unfolded: the next capture opens unfolded too.
    pub toolbar_unfolded: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            instant_shortcut: Shortcut {
                modifiers: 0,
                key: 145,
            },
            start_with_windows: true,
            capture_stroke_width: 3,
            capture_eraser_size: crate::ERASER_SIZE as u32,
            live_capture: false,
            modifiers: 0,
            key: 0,
            legacy_style: false,
            folder: PathBuf::new(),
            save_format: SaveFormat::default(),
            presentation: PresentationSettings::default(),
            last_custom_color: [237, 79, 92],
            // Alt+B.
            board_shortcut: Shortcut {
                modifiers: 1,
                key: 0x42,
            },
            board_visible: false,
            language: LanguageChoice::Auto,
            wheel_learned: false,
            mini_toolbar: false,
            toolbar_unfolded: false,
        }
    }
}
/// Header of the settings file, followed by the format version.
const HEADER: &str = "shotoshot-settings-";
/// Header written before the product was renamed to Shotoshot (2026-09-28); still read.
const LEGACY_HEADER: &str = "shotshot-settings-";

/// Version named by a current or legacy settings header.
fn header_version(header: &str) -> Option<&str> {
    header
        .strip_prefix(HEADER)
        .or_else(|| header.strip_prefix(LEGACY_HEADER))
}
impl Settings {
    pub fn brush(&self) -> Brush {
        Brush {
            stroke_width: self.capture_stroke_width,
            eraser_size: self.capture_eraser_size,
        }
    }
    pub fn load_from(path: &Path) -> std::result::Result<Self, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::decode(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.to_string()),
        }
    }
    pub fn decode(text: &str) -> std::result::Result<Self, String> {
        let fields: Vec<_> = text.lines().collect();
        let version = fields.first().copied().and_then(header_version);
        let expected_len = match version {
            Some("1") => 5,
            Some("2") => 6,
            Some("3") => 9,
            Some("4") => 10,
            Some("5") => 13,
            Some("6") => 15,
            Some("7") => 17,
            Some("8") => 18,
            Some("9") => 20,
            Some("10" | "11") => 22,
            Some("12") => 24,
            Some("13") => 25,
            Some("14") => 27,
            Some("15") => 28,
            Some("16") => 29,
            Some("17") => 30,
            Some("18") => 31,
            Some("19") => 32,
            Some("20") => 33,
            Some("21") => 34,
            _ => 0,
        };
        if fields.len() != expected_len {
            return Err(format!(
                "unsupported settings: {} fields for this version",
                fields.len()
            ));
        }
        if fields.len() >= 6 && !matches!(fields[5], "0" | "1") {
            return Err("Invalid live capture setting".into());
        }
        let mut presentation = if fields.len() >= 9 {
            PresentationSettings::decode(fields[6], fields[7], fields[8])?
        } else {
            PresentationSettings::default()
        };
        if fields.len() >= 10 {
            presentation.input_position = DisplayPosition::decode(fields[9])?;
        }
        if fields.len() >= 13 {
            for (shortcut, field) in presentation.shape_shortcuts.iter_mut().zip(&fields[10..13]) {
                *shortcut = Shortcut::decode(field)?;
            }
            presentation.validate_shape_shortcuts().map_err(refused)?;
            // An arrow left on the old default follows the editor's letter, unless Alt+A
            // already draws another shape. Formats 11 and later keep an Alt+F chosen since.
            let arrow = DEFAULT_SHAPE_SHORTCUTS[0];
            // PITFALL: every format from 11 on is listed; the next version goes here too, or
            // its users' Alt+F arrow would be migrated away on every load.
            if !matches!(
                version,
                Some("11" | "12" | "13" | "14" | "15" | "16" | "17" | "18" | "19" | "20" | "21")
            ) && presentation.shape_shortcuts[0] == OLD_ARROW_SHORTCUT
                && !presentation.shape_shortcuts.contains(&arrow)
            {
                presentation.shape_shortcuts[0] = arrow;
            }
        }
        if fields.len() >= 15 {
            presentation.stroke_width = fields[13]
                .parse::<u32>()
                .ok()
                .filter(|v| (1..=40).contains(v) && v.to_string() == fields[13])
                .ok_or("Invalid presentation stroke width")?;
            presentation.arrow_size = match fields[14] {
                "16" => 16,
                "28" => 28,
                "40" => 40,
                _ => return Err("Invalid presentation arrow size".into()),
            };
        }
        if fields.len() >= 17 {
            presentation.input_offset = if fields[15] == "-" {
                None
            } else {
                let (x, y) = fields[15]
                    .split_once(',')
                    .ok_or("Invalid input display offset")?;
                let coordinate = |value: &str| -> std::result::Result<u16, String> {
                    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                        return Err("Invalid input display offset".into());
                    }
                    let parsed = value
                        .parse::<u16>()
                        .map_err(|_| "Invalid input display offset")?;
                    if parsed > 10000 {
                        return Err("Invalid input display offset".into());
                    }
                    Ok(parsed)
                };
                Some([coordinate(x)?, coordinate(y)?])
            };
            // The retired lock flag is still checked, so a damaged line stays refused. The
            // diagram now unlocks only for a move asked from the menu, never at startup.
            if !matches!(fields[16], "0" | "1") {
                return Err("Invalid input display lock".into());
            }
        }
        if fields.len() >= 30 {
            presentation.pointer = crate::pointer::PointerSettings::decode(fields[29])?;
        }
        presentation.obs_mirror = match fields.get(33) {
            None | Some(&"0") => false,
            Some(&"1") => true,
            Some(_) => return Err("Invalid OBS mirror setting".into()),
        };
        if fields.len() >= 24 {
            // The retired keyboard layout (full, compact, gaming) is still checked, so a damaged
            // line stays refused; the ghost diagram always shows the whole keyboard.
            if !matches!(fields[22], "0" | "1" | "2") {
                return Err("Invalid input display layout".into());
            }
            presentation.input_scale = INPUT_SCALES
                .into_iter()
                .find(|scale| scale.to_string() == fields[23])
                .ok_or("Invalid input display size")?;
        }
        let parse = |s: &str| {
            s.parse::<u32>()
                .map_err(|_| "Invalid settings number".to_string())
        };
        let key = parse(fields[2])?;
        let modifiers = parse(fields[1])?;
        if key > 254 || modifiers > 15 || !matches!(fields[3], "0" | "1") {
            return Err("Invalid shortcut or drawing style".into());
        }
        let units: std::result::Result<Vec<u16>, _> = fields[4]
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| u16::from_str_radix(s, 16))
            .collect();
        let folder = decode_folder(&units.map_err(|_| "Invalid folder encoding")?)?;
        let instant_shortcut = fields
            .get(20)
            .map(|v| Shortcut::decode(v))
            .transpose()?
            .unwrap_or(Self::default().instant_shortcut);
        if instant_shortcut.modifiers > 15 || !(1..=254).contains(&instant_shortcut.key) {
            return Err("Invalid instant capture shortcut".into());
        }
        let board_shortcut = match fields.get(25) {
            None => Self::default().board_shortcut,
            Some(value) => {
                let shortcut = Shortcut::decode(value)?;
                Self::validate_board_shortcut(&shortcut).map_err(refused)?;
                shortcut
            }
        };
        // Former capture sound switch: the sound is always on, the slot stays for compatibility.
        if !matches!(fields.get(21), None | Some(&"0" | &"1")) {
            return Err("Invalid instant capture sound setting".into());
        }
        let board_visible = match fields.get(26) {
            None | Some(&"0") => false,
            Some(&"1") => true,
            Some(_) => return Err("Invalid board visibility".into()),
        };
        // An unknown or empty tag is not damage: it means the system language.
        let language = fields
            .get(28)
            .map_or(LanguageChoice::Auto, |value| LanguageChoice::decode(value));
        let wheel_learned = match fields.get(30) {
            None | Some(&"0") => false,
            Some(&"1") => true,
            Some(_) => return Err("Invalid wheel hint setting".into()),
        };
        let mini_toolbar = match fields.get(31) {
            None | Some(&"0") => false,
            Some(&"1") => true,
            Some(_) => return Err("Invalid mini toolbar setting".into()),
        };
        let toolbar_unfolded = match fields.get(32) {
            None | Some(&"0") => false,
            Some(&"1") => true,
            Some(_) => return Err("Invalid mini toolbar fold".into()),
        };
        Ok(Self {
            instant_shortcut,
            start_with_windows: match fields.get(18) {
                None | Some(&"1") => true,
                Some(&"0") => false,
                _ => return Err("Invalid startup setting".into()),
            },
            capture_stroke_width: match fields.get(19) {
                None => 3,
                Some(value) => value
                    .parse::<u32>()
                    .ok()
                    .filter(|v| (1..=40).contains(v) && v.to_string() == *value)
                    .ok_or("Invalid capture stroke width")?,
            },
            capture_eraser_size: match fields.get(27) {
                None => crate::ERASER_SIZE as u32,
                Some(value) => value
                    .parse::<u32>()
                    .ok()
                    .filter(|v| (4..=128).contains(v) && v.to_string() == *value)
                    .ok_or("Invalid capture eraser size")?,
            },
            live_capture: fields.get(5) == Some(&"1"),
            key,
            modifiers,
            legacy_style: fields[3] == "1",
            folder,
            save_format: fields
                .get(24)
                .map(|v| SaveFormat::decode(v))
                .transpose()?
                .unwrap_or_default(),
            presentation,
            last_custom_color: if let Some(color) = fields.get(17) {
                if color.len() != 6 || !color.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return Err("Invalid custom color".into());
                }
                let value = u32::from_str_radix(color, 16).map_err(|_| "Invalid custom color")?;
                let [_, red, green, blue] = value.to_be_bytes();
                [red, green, blue]
            } else {
                Self::default().last_custom_color
            },
            board_shortcut,
            board_visible,
            language,
            wheel_learned,
            mini_toolbar,
            toolbar_unfolded,
        })
    }
    /// A board shortcut needs Ctrl or Alt so that typing never toggles the board. A refusal
    /// is the text the user reads: one of `BOARD_SHORTCUT_REFUSALS`.
    pub fn validate_board_shortcut(shortcut: &Shortcut) -> std::result::Result<(), Key> {
        if shortcut.modifiers > 15 || !(1..=254).contains(&shortcut.key) {
            return Err(Key::SetupInvalidBoard);
        }
        if shortcut.modifiers & 3 == 0 {
            return Err(Key::SetupBoardNeedsModifier);
        }
        if shortcut.modifiers == 3 && shortcut.key == 27 {
            return Err(Key::SetupReserved);
        }
        Ok(())
    }
    pub fn encode(&self) -> String {
        let folder = encode_folder(&self.folder)
            .iter()
            .map(|v| format!("{v:x}"))
            .collect::<Vec<_>>()
            .join(",");
        let color = |[red, green, blue]: [u8; 3]| format!("{red:02x}{green:02x}{blue:02x}");
        format!(
            "{HEADER}21\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{},{}\n{},{}\n{},{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{},{}\n{}\n{}\n{}\n{}\n{},{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n",
            self.modifiers,
            self.key,
            u8::from(self.legacy_style),
            folder,
            u8::from(self.live_capture),
            self.presentation.duration_ms,
            color(self.presentation.halo_color),
            color(self.presentation.shape_color),
            self.presentation.input_position.index(),
            self.presentation.shape_shortcuts[0].modifiers,
            self.presentation.shape_shortcuts[0].key,
            self.presentation.shape_shortcuts[1].modifiers,
            self.presentation.shape_shortcuts[1].key,
            self.presentation.shape_shortcuts[2].modifiers,
            self.presentation.shape_shortcuts[2].key,
            self.presentation.stroke_width,
            self.presentation.arrow_size,
            self.presentation
                .input_offset
                .map_or_else(|| "-".to_string(), |[x, y]| format!("{x},{y}")),
            // Retired lock flag, written locked so an older version keeps the diagram still.
            1,
            color(self.last_custom_color),
            u8::from(self.start_with_windows),
            self.capture_stroke_width,
            self.instant_shortcut.modifiers,
            self.instant_shortcut.key,
            1,
            // Retired keyboard layout, written full so an older version shows the whole keyboard.
            0,
            self.presentation.input_scale,
            self.save_format.index(),
            self.board_shortcut.modifiers,
            self.board_shortcut.key,
            u8::from(self.board_visible),
            self.capture_eraser_size,
            self.language.encode(),
            self.presentation.pointer.encode(),
            u8::from(self.wheel_learned),
            u8::from(self.mini_toolbar),
            u8::from(self.toolbar_unfolded),
            u8::from(self.presentation.obs_mirror)
        )
    }
    /// Changes only the editor's custom color, preserving the other stored preferences.
    pub fn save_custom_color_to(
        path: &Path,
        color: [u8; 3],
        replace: Replace,
    ) -> std::result::Result<(), String> {
        Self::save_editor_to(path, Some(color), None, None, None, replace)
    }
    /// Changes only whether the board is shown, preserving the other stored preferences.
    pub fn save_board_visible_to(
        path: &Path,
        visible: bool,
        replace: Replace,
    ) -> std::result::Result<(), String> {
        let mut current = Self::load_from(path)?;
        current.board_visible = visible;
        current.write_to(path, replace)
    }
    /// Records that the user has resized with the wheel, preserving the other preferences.
    pub fn save_wheel_learned_to(path: &Path, replace: Replace) -> std::result::Result<(), String> {
        let mut current = Self::load_from(path)?;
        current.wheel_learned = true;
        current.write_to(path, replace)
    }
    /// Remembers whether the mini toolbar was left unfolded, preserving the other preferences.
    pub fn save_toolbar_unfolded_to(
        path: &Path,
        unfolded: bool,
        replace: Replace,
    ) -> std::result::Result<(), String> {
        let mut current = Self::load_from(path)?;
        current.toolbar_unfolded = unfolded;
        current.write_to(path, replace)
    }
    /// Merges only fields changed by an editor into the latest resident preferences.
    pub fn save_editor_to(
        path: &Path,
        color: Option<[u8; 3]>,
        width: Option<u32>,
        eraser: Option<u32>,
        destination: Option<(&Path, SaveFormat)>,
        replace: Replace,
    ) -> std::result::Result<(), String> {
        let mut current = Self::load_from(path)?;
        if let Some(color) = color {
            current.last_custom_color = color;
        }
        if let Some(width) = width {
            current.capture_stroke_width = width;
        }
        if let Some(eraser) = eraser {
            current.capture_eraser_size = eraser;
        }
        if let Some((folder, format)) = destination {
            current.folder = folder.to_path_buf();
            current.save_format = format;
        }
        current.write_to(path, replace)
    }
    /// Writes these settings, keeping the stored custom color and stroke width unless
    /// given: editors own those two and may have saved them since this copy was loaded.
    /// The eraser size and the board visibility are never taken from this copy: only
    /// `save_editor_to` and `save_board_visible_to` change them.
    pub fn save_to(
        &self,
        path: &Path,
        color: Option<[u8; 3]>,
        width: Option<u32>,
        replace: Replace,
    ) -> std::result::Result<(), String> {
        let stored = Self::load_from(path)?;
        let mut merged = self.clone();
        merged.last_custom_color = color.unwrap_or(stored.last_custom_color);
        merged.capture_stroke_width = width.unwrap_or(stored.capture_stroke_width);
        merged.capture_eraser_size = stored.capture_eraser_size;
        merged.board_visible = stored.board_visible;
        merged.write_to(path, replace)
    }
    fn write_to(&self, path: &Path, replace: Replace) -> std::result::Result<(), String> {
        self.presentation
            .validate_shape_shortcuts()
            .map_err(refused)?;
        Self::validate_board_shortcut(&self.board_shortcut).map_err(refused)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let temp = path.with_extension(format!("{}.tmp", std::process::id()));
        std::fs::write(&temp, self.encode()).map_err(|e| e.to_string())?;
        replace(&temp, path)
    }
}

/// Moves the written temporary file over the settings file, atomically.
pub(crate) type Replace = fn(&Path, &Path) -> std::result::Result<(), String>;

/// Portable replace: `rename` is atomic on Unix and replaces on Windows.
pub fn rename(from: &Path, to: &Path) -> std::result::Result<(), String> {
    std::fs::rename(from, to).map_err(|e| e.to_string())
}

// The folder is stored as UTF-16 code units, the Windows native form.
#[cfg(windows)]
fn encode_folder(folder: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    folder.as_os_str().encode_wide().collect()
}
#[cfg(windows)]
fn decode_folder(units: &[u16]) -> std::result::Result<PathBuf, String> {
    use std::os::windows::ffi::OsStringExt;
    Ok(PathBuf::from(std::ffi::OsString::from_wide(units)))
}
#[cfg(not(windows))]
fn encode_folder(folder: &Path) -> Vec<u16> {
    folder.to_string_lossy().encode_utf16().collect()
}
#[cfg(not(windows))]
fn decode_folder(units: &[u16]) -> std::result::Result<PathBuf, String> {
    String::from_utf16(units)
        .map(PathBuf::from)
        .map_err(|_| "Invalid folder encoding".into())
}
#[cfg(test)]
mod tests;
