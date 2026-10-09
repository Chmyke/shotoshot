//! ROLE: The settings file format shared by every platform: types, defaults, versioned
//! decoding and the merge-then-replace save.
//! OWNER: Each platform supplies the file path, a cross-process lock and the final replace.
//! INVARIANTS: Keys are Windows virtual-key codes on every platform; a damaged file is
//! reported and left untouched.
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
/// Which part of the keyboard the presentation diagram shows.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum InputLayout {
    /// Every key, numeric pad included.
    #[default]
    Full,
    /// Without the numeric pad.
    Compact,
    /// The left-hand block games use, with a shortened space bar.
    Gaming,
}
impl InputLayout {
    pub const ALL: [Self; 3] = [Self::Full, Self::Compact, Self::Gaming];
    pub fn index(self) -> usize {
        match self {
            Self::Full => 0,
            Self::Compact => 1,
            Self::Gaming => 2,
        }
    }
    fn decode(value: &str) -> std::result::Result<Self, String> {
        match value {
            "0" => Ok(Self::Full),
            "1" => Ok(Self::Compact),
            "2" => Ok(Self::Gaming),
            _ => Err("Invalid input display layout".into()),
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
    pub input_locked: bool,
    pub input_layout: InputLayout,
    /// One of `INPUT_SCALES`.
    pub input_scale: u32,
    pub duration_ms: u32,
    pub halo_color: [u8; 3],
    pub shape_color: [u8; 3],
    pub stroke_width: u32,
    pub arrow_size: u32,
}
impl Default for PresentationSettings {
    fn default() -> Self {
        Self {
            stroke_width: 5,
            arrow_size: 28,
            shape_shortcuts: DEFAULT_SHAPE_SHORTCUTS,
            input_position: DisplayPosition::default(),
            input_offset: None,
            input_locked: false,
            input_layout: InputLayout::default(),
            input_scale: 100,
            duration_ms: 3000,
            halo_color: [255, 211, 64],
            shape_color: [237, 79, 92],
        }
    }
}
impl PresentationSettings {
    pub fn validate_shape_shortcuts(&self) -> std::result::Result<(), String> {
        for (index, shortcut) in self.shape_shortcuts.iter().enumerate() {
            if shortcut.modifiers > 15 || !(1..=254).contains(&shortcut.key) {
                return Err("Invalid shape shortcut".into());
            }
            if shortcut.modifiers & 3 == 0 {
                return Err(
                    "Choose a shortcut with Ctrl or Alt to avoid triggering shapes while typing."
                        .into(),
                );
            }
            if shortcut.modifiers == 3 && shortcut.key == 27 {
                return Err("Ctrl+Alt+Escape is reserved".into());
            }
            if self.shape_shortcuts[..index].contains(shortcut) {
                return Err("Duplicate shape shortcut".into());
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
            input_locked: false,
            input_layout: InputLayout::default(),
            input_scale: 100,
            duration_ms,
            halo_color: color(halo)?,
            shape_color: color(shape)?,
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub instant_shortcut: Shortcut,
    pub instant_sound: bool,
    pub start_with_windows: bool,
    pub capture_stroke_width: u32,
    pub live_capture: bool,
    pub modifiers: u32,
    pub key: u32,
    // Legacy file-format slot, preserved for compatibility only.
    pub legacy_style: bool,
    pub folder: PathBuf,
    pub save_format: SaveFormat,
    pub presentation: PresentationSettings,
    pub last_custom_color: [u8; 3],
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            instant_shortcut: Shortcut {
                modifiers: 0,
                key: 145,
            },
            instant_sound: true,
            start_with_windows: true,
            capture_stroke_width: 3,
            live_capture: false,
            modifiers: 0,
            key: 0,
            legacy_style: false,
            folder: PathBuf::new(),
            save_format: SaveFormat::default(),
            presentation: PresentationSettings::default(),
            last_custom_color: [237, 79, 92],
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
            _ => 0,
        };
        if fields.len() != expected_len {
            return Err("Unsupported or damaged settings. The original file was preserved.".into());
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
            presentation.validate_shape_shortcuts()?;
            // An arrow left on the old default follows the editor's letter, unless Alt+A
            // already draws another shape. Formats 11 and later keep an Alt+F chosen since.
            let arrow = DEFAULT_SHAPE_SHORTCUTS[0];
            if !matches!(version, Some("11" | "12" | "13"))
                && presentation.shape_shortcuts[0] == OLD_ARROW_SHORTCUT
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
            presentation.input_locked = match fields[16] {
                "0" => false,
                "1" => true,
                _ => return Err("Invalid input display lock".into()),
            };
        }
        if fields.len() >= 24 {
            presentation.input_layout = InputLayout::decode(fields[22])?;
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
        Ok(Self {
            instant_shortcut,
            instant_sound: match fields.get(21) {
                None | Some(&"1") => true,
                Some(&"0") => false,
                _ => return Err("Invalid instant capture sound setting".into()),
            },
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
        })
    }
    pub fn encode(&self) -> String {
        let folder = encode_folder(&self.folder)
            .iter()
            .map(|v| format!("{v:x}"))
            .collect::<Vec<_>>()
            .join(",");
        let color = |[red, green, blue]: [u8; 3]| format!("{red:02x}{green:02x}{blue:02x}");
        format!(
            "{HEADER}13\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{},{}\n{},{}\n{},{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{},{}\n{}\n{}\n{}\n{}\n",
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
            u8::from(self.presentation.input_locked),
            color(self.last_custom_color),
            u8::from(self.start_with_windows),
            self.capture_stroke_width,
            self.instant_shortcut.modifiers,
            self.instant_shortcut.key,
            u8::from(self.instant_sound),
            self.presentation.input_layout.index(),
            self.presentation.input_scale,
            self.save_format.index()
        )
    }
    /// Changes only the editor's custom color, preserving the other stored preferences.
    pub fn save_custom_color_to(
        path: &Path,
        color: [u8; 3],
        replace: Replace,
    ) -> std::result::Result<(), String> {
        Self::save_editor_to(path, Some(color), None, None, replace)
    }
    /// Merges only fields changed by an editor into the latest resident preferences.
    pub fn save_editor_to(
        path: &Path,
        color: Option<[u8; 3]>,
        width: Option<u32>,
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
        if let Some((folder, format)) = destination {
            current.folder = folder.to_path_buf();
            current.save_format = format;
        }
        current.write_to(path, replace)
    }
    /// Writes these settings, keeping the stored custom color and stroke width unless
    /// given: editors own those two and may have saved them since this copy was loaded.
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
        merged.write_to(path, replace)
    }
    fn write_to(&self, path: &Path, replace: Replace) -> std::result::Result<(), String> {
        self.presentation.validate_shape_shortcuts()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let temp = path.with_extension(format!("{}.tmp", std::process::id()));
        std::fs::write(&temp, self.encode()).map_err(|e| e.to_string())?;
        replace(&temp, path)
    }
}

/// Moves the written temporary file over the settings file, atomically.
pub type Replace = fn(&Path, &Path) -> std::result::Result<(), String>;

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
mod tests {
    #[test]
    fn writes_the_shotoshot_header_and_still_reads_the_pre_rename_one() {
        let settings = Settings {
            last_custom_color: [1, 2, 3],
            ..Default::default()
        };
        let encoded = settings.encode();
        assert!(encoded.starts_with("shotoshot-settings-13\n"));
        let legacy = encoded.replacen("shotoshot-settings-", "shotshot-settings-", 1);
        assert_eq!(Settings::decode(&legacy).unwrap(), settings);
        let oldest = Settings::decode("shotshot-settings-1\n3\n65\n1\n43,3a,2f,e9\n").unwrap();
        assert_eq!((oldest.modifiers, oldest.key), (3, 65));
        assert!(Settings::decode(&encoded.replacen("shotoshot-", "other-", 1)).is_err());
        assert!(Settings::decode(&encoded.replacen("-13\n", "-14\n", 1)).is_err());
    }
    #[test]
    fn save_format_roundtrips_and_older_files_keep_png() {
        let fields: Vec<String> = Settings::default()
            .encode()
            .lines()
            .map(str::to_owned)
            .collect();
        assert_eq!(&fields[24..], &["0"]);
        let mut old = fields[..24].to_vec();
        old[0] = "shotoshot-settings-12".into();
        let migrated = Settings::decode(&old.join("\n")).unwrap();
        assert_eq!(migrated.save_format, SaveFormat::Png);
        for format in SaveFormat::ALL {
            let settings = Settings {
                save_format: format,
                ..Default::default()
            };
            assert_eq!(Settings::decode(&settings.encode()).unwrap(), settings);
        }
        for invalid in ["2", "jpg", "00", " 1"] {
            let mut damaged = fields.clone();
            damaged[24] = invalid.into();
            assert!(
                Settings::decode(&damaged.join("\n")).is_err(),
                "accepted save format {invalid:?}"
            );
        }
    }
    #[test]
    fn save_format_follows_the_file_extension() {
        let of = |name: &str| SaveFormat::of(Path::new(name));
        assert_eq!(of("C:/été/Shot 001.PNG"), Some(SaveFormat::Png));
        assert_eq!(of("photo.jpg"), Some(SaveFormat::Jpeg));
        assert_eq!(of("photo.JPEG"), Some(SaveFormat::Jpeg));
        assert_eq!(of("notes.txt"), None);
        assert_eq!(of("png"), None);
        for format in SaveFormat::ALL {
            let path = Path::new("Shot 001").with_extension(format.extension());
            assert_eq!(SaveFormat::of(&path), Some(format));
        }
    }
    #[test]
    fn input_layout_and_size_roundtrip_and_older_files_keep_the_full_keyboard() {
        let fields: Vec<String> = Settings::default()
            .encode()
            .lines()
            .map(str::to_owned)
            .collect();
        assert_eq!(&fields[22..24], &["0", "100"]);
        for version in ["10", "11"] {
            let mut old = fields[..22].to_vec();
            old[0] = format!("shotoshot-settings-{version}");
            let migrated = Settings::decode(&old.join("\n")).unwrap();
            assert_eq!(migrated.presentation.input_layout, InputLayout::Full);
            assert_eq!(migrated.presentation.input_scale, 100);
        }
        for layout in InputLayout::ALL {
            for scale in INPUT_SCALES {
                let mut settings = Settings::default();
                settings.presentation.input_layout = layout;
                settings.presentation.input_scale = scale;
                assert_eq!(Settings::decode(&settings.encode()).unwrap(), settings);
            }
        }
        for (index, invalid) in [
            (22, "3"),
            (22, "full"),
            (22, "00"),
            (23, "0"),
            (23, "125"),
            (23, "0100"),
            (23, "200 "),
        ] {
            let mut damaged = fields.clone();
            damaged[index] = invalid.into();
            assert!(
                Settings::decode(&damaged.join("\n")).is_err(),
                "accepted field {index}: {invalid:?}"
            );
        }
    }
    #[test]
    fn instant_capture_migration_defaults_and_custom_settings_roundtrip() {
        let mut old: Vec<String> = Settings::default()
            .encode()
            .lines()
            .take(20)
            .map(str::to_owned)
            .collect();
        old[0] = "shotoshot-settings-9".into();
        let migrated = Settings::decode(&old.join("\n")).unwrap().encode();
        let fields: Vec<_> = migrated.lines().collect();
        assert_eq!(fields[0], "shotoshot-settings-13");
        assert_eq!(&fields[20..22], &["0,145", "1"]);
        let mut custom: Vec<_> = fields.iter().map(|v| v.to_string()).collect();
        custom[20] = "6,122".into();
        custom[21] = "0".into();
        assert_eq!(
            Settings::decode(&custom.join("\n"))
                .unwrap()
                .encode()
                .lines()
                .collect::<Vec<_>>(),
            custom
        );
        custom[20] = "0,0".into();
        assert!(Settings::decode(&custom.join("\n")).is_err());
    }
    use super::*;
    #[test]
    fn startup_and_last_width_survive_reload_without_losing_diagram_position() {
        let path =
            std::env::temp_dir().join(format!("shotoshot-last-width-{}.txt", std::process::id()));
        let mut resident = Settings {
            start_with_windows: false,
            ..Default::default()
        };
        resident.presentation.input_offset = Some([1234, 8765]);
        resident.presentation.input_locked = true;
        resident.save_to(&path, None, Some(7), rename).unwrap();
        resident.save_to(&path, None, None, rename).unwrap();
        Settings::save_custom_color_to(&path, [10, 20, 30], rename).unwrap();
        let loaded = Settings::load_from(&path).unwrap();
        assert!(!loaded.start_with_windows);
        assert_eq!(loaded.capture_stroke_width, 7);
        assert_eq!(loaded.presentation.input_offset, Some([1234, 8765]));
        assert!(loaded.presentation.input_locked);
        std::fs::remove_file(path).unwrap();
        assert!(Settings::default().start_with_windows);
        let old = Settings::default()
            .encode()
            .lines()
            .take(18)
            .collect::<Vec<_>>()
            .join("\n")
            .replacen("settings-13", "settings-8", 1);
        assert!(Settings::decode(&old).unwrap().start_with_windows);
    }
    #[test]
    fn custom_color_roundtrips_and_old_versions_keep_the_default() {
        let settings = Settings {
            last_custom_color: [0, 128, 255],
            ..Default::default()
        };
        assert_eq!(Settings::decode(&settings.encode()).unwrap(), settings);
        for (version, count) in [(1, 5), (2, 6), (3, 9), (4, 10), (5, 13), (6, 15), (7, 17)] {
            let encoded = settings.encode();
            let mut fields: Vec<_> = encoded.lines().take(count).map(str::to_owned).collect();
            fields[0] = format!("shotoshot-settings-{version}");
            let migrated = Settings::decode(&(fields.join("\n") + "\n")).unwrap();
            assert_eq!(migrated.last_custom_color, [237, 79, 92]);
            assert_eq!(Settings::decode(&migrated.encode()).unwrap(), migrated);
        }
        for invalid in ["", "#123456", "12345", "1234567", "12gg34", "é1234"] {
            let encoded = settings.encode();
            let mut fields: Vec<_> = encoded.lines().collect();
            fields[17] = invalid;
            assert!(Settings::decode(&(fields.join("\n") + "\n")).is_err());
        }
    }
    #[test]
    fn editor_field_merge_keeps_current_resident_preferences_and_reports_failure() {
        let path =
            std::env::temp_dir().join(format!("shotoshot-editor-merge-{}.txt", std::process::id()));
        let resident = Settings {
            key: 77,
            live_capture: true,
            start_with_windows: false,
            ..Default::default()
        };
        resident.save_to(&path, None, Some(9), rename).unwrap();
        let folder = std::env::temp_dir().join("editor-last-folder");
        let destination = Some((folder.as_path(), SaveFormat::Jpeg));
        Settings::save_editor_to(&path, Some([10, 20, 30]), None, destination, rename).unwrap();
        let stored = Settings::load_from(&path).unwrap();
        assert_eq!(stored.save_format, SaveFormat::Jpeg);
        assert_eq!(stored.key, 77);
        assert!(stored.live_capture);
        assert!(!stored.start_with_windows);
        assert_eq!(stored.capture_stroke_width, 9);
        assert_eq!(stored.last_custom_color, [10, 20, 30]);
        assert_eq!(stored.folder, folder);
        let before = std::fs::read(&path).unwrap();
        let failed = Settings::save_editor_to(&path, None, Some(17), None, |_, _| {
            Err("replace failed".into())
        });
        assert_eq!(failed.unwrap_err(), "replace failed");
        assert_eq!(std::fs::read(&path).unwrap(), before);
        std::fs::write(&path, "damaged settings").unwrap();
        assert!(Settings::save_editor_to(&path, None, Some(17), None, rename).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "damaged settings");
        std::fs::remove_file(path.with_extension(format!("{}.tmp", std::process::id()))).unwrap();
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn stale_resident_save_preserves_child_color_and_color_save_preserves_preferences() {
        let path = std::env::temp_dir().join(format!(
            "shotoshot-color-settings-{}.txt",
            std::process::id()
        ));
        let mut resident = Settings::default();
        resident.save_to(&path, None, None, rename).unwrap();
        Settings::save_custom_color_to(&path, [12, 34, 56], rename).unwrap();
        resident.key = 65;
        resident.save_to(&path, None, None, rename).unwrap();
        let stored = Settings::decode(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(stored.key, 65);
        assert_eq!(stored.last_custom_color, [12, 34, 56]);
        Settings::save_custom_color_to(&path, [78, 90, 12], rename).unwrap();
        let stored = Settings::decode(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(stored.key, 65);
        assert_eq!(stored.last_custom_color, [78, 90, 12]);
        std::fs::write(&path, "damaged settings").unwrap();
        assert!(resident.save_to(&path, None, None, rename).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "damaged settings");
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn free_input_position_and_lock_roundtrip_and_validate() {
        for offset in [None, Some([0, 0]), Some([10000, 10000]), Some([3210, 7890])] {
            for locked in [false, true] {
                let mut settings = Settings::default();
                settings.presentation.input_offset = offset;
                settings.presentation.input_locked = locked;
                assert_eq!(Settings::decode(&settings.encode()).unwrap(), settings);
            }
        }
        let valid = Settings::default().encode();
        for (index, values) in [
            (
                15,
                &[
                    "", "10001,0", "0,10001", "65536,0", "-1,0", "+1,0", "1,", "1,2,3", "x,0",
                ][..],
            ),
            (16, &["", "2", "true", "01"][..]),
        ] {
            for invalid in values {
                let mut fields: Vec<_> = valid.lines().collect();
                fields[index] = invalid;
                assert!(
                    Settings::decode(&(fields.join("\n") + "\n")).is_err(),
                    "accepted {index}: {invalid:?}"
                );
            }
        }
    }
    #[test]
    fn version_six_keeps_custom_settings_and_defaults_drag_preferences() {
        let text = "shotoshot-settings-6\n3\n65\n1\n43,3a,2f,e9\n1\n5000\n010203\n040506\n2\n6,112\n2,113\n9,114\n12\n40\n";
        let settings = Settings::decode(text).unwrap();
        assert_eq!(settings.presentation.input_offset, None);
        assert!(!settings.presentation.input_locked);
        assert_eq!(settings.presentation.stroke_width, 12);
        assert_eq!(settings.presentation.arrow_size, 40);
        assert_eq!(
            settings.presentation.input_position,
            DisplayPosition::TopRight
        );
        assert_eq!(
            settings.presentation.shape_shortcuts[0],
            Shortcut {
                modifiers: 6,
                key: 112
            }
        );
        assert_eq!(Settings::decode(&settings.encode()).unwrap(), settings);
    }
    #[test]
    fn stroke_preferences_roundtrip_and_reject_unsupported_values() {
        for width in [2, 3, 5, 8, 12] {
            for size in [16, 28, 40] {
                let mut settings = Settings::default();
                settings.presentation.stroke_width = width;
                settings.presentation.arrow_size = size;
                assert_eq!(Settings::decode(&settings.encode()).unwrap(), settings);
            }
        }
        let valid = Settings::default().encode();
        for (index, values) in [
            (13, &["0", "41", "-5", "05", "+5", ""][..]),
            (14, &["0", "15", "29", "41", "028", "+28", ""][..]),
        ] {
            for invalid in values {
                let mut fields: Vec<_> = valid.lines().collect();
                fields[index] = invalid;
                assert!(
                    Settings::decode(&(fields.join("\n") + "\n")).is_err(),
                    "accepted field {index}: {invalid:?}"
                );
            }
        }
    }
    #[test]
    fn version_five_keeps_custom_shortcuts_and_adds_stroke_defaults() {
        let text = "shotoshot-settings-5\n3\n65\n1\n43,3a,2f,e9\n1\n5000\n010203\n040506\n2\n6,112\n2,113\n9,114\n";
        let settings = Settings::decode(text).unwrap();
        assert_eq!(settings.presentation.stroke_width, 5);
        assert_eq!(settings.presentation.arrow_size, 28);
        assert_eq!(
            settings.presentation.input_position,
            DisplayPosition::TopRight
        );
        assert_eq!(
            settings.presentation.shape_shortcuts,
            [
                Shortcut {
                    modifiers: 6,
                    key: 112
                },
                Shortcut {
                    modifiers: 2,
                    key: 113
                },
                Shortcut {
                    modifiers: 9,
                    key: 114
                }
            ]
        );
        assert_eq!(settings.presentation.duration_ms, 5000);
        assert_eq!(settings.presentation.halo_color, [1, 2, 3]);
        assert_eq!(settings.presentation.shape_color, [4, 5, 6]);
        assert_eq!(Settings::decode(&settings.encode()).unwrap(), settings);
    }
    #[test]
    fn version_four_preserves_position_and_adds_default_shape_shortcuts() {
        let text = "shotoshot-settings-4\n3\n65\n1\n43,3a,2f,e9\n1\n5000\n010203\n040506\n2\n";
        let settings = Settings::decode(text).unwrap();
        assert_eq!(
            settings.presentation.input_position,
            DisplayPosition::TopRight
        );
        assert_eq!(
            settings.presentation.shape_shortcuts,
            DEFAULT_SHAPE_SHORTCUTS
        );
        assert_eq!(settings.presentation.duration_ms, 5000);
        assert_eq!(settings.presentation.halo_color, [1, 2, 3]);
        assert_eq!(settings.presentation.shape_color, [4, 5, 6]);
        assert_eq!(Settings::decode(&settings.encode()).unwrap(), settings);
    }
    #[test]
    fn older_formats_move_an_untouched_arrow_shortcut_to_alt_a() {
        let stored = |version: &str, arrow: &str, rectangle: &str| {
            let mut fields: Vec<String> = Settings::default()
                .encode()
                .lines()
                .map(str::to_owned)
                .collect();
            fields[0] = format!("shotoshot-settings-{version}");
            fields[10] = arrow.into();
            fields[11] = rectangle.into();
            // Formats 10 and 11 end before the input layout and size, 12 before the save format.
            match version {
                "10" | "11" => fields.truncate(22),
                "12" => fields.truncate(24),
                _ => {}
            }
            let settings = Settings::decode(&(fields.join("\n") + "\n")).unwrap();
            settings.presentation.shape_shortcuts[0]
        };
        let alt = |key| Shortcut { modifiers: 1, key };
        assert_eq!(DEFAULT_SHAPE_SHORTCUTS, [alt(0x41), alt(0x52), alt(0x43)]);
        assert_eq!(stored("10", "1,70", "1,82"), alt(0x41));
        assert_eq!(
            stored("10", "6,112", "1,82"),
            Shortcut {
                modifiers: 6,
                key: 112
            },
            "a chosen shortcut is kept"
        );
        assert_eq!(
            stored("10", "1,70", "1,65"),
            alt(0x46),
            "Alt+A already draws another shape"
        );
        assert_eq!(
            stored("11", "1,70", "1,82"),
            alt(0x46),
            "Alt+F chosen again after the change is kept"
        );
        assert_eq!(stored("12", "1,70", "1,82"), alt(0x46));
        assert_eq!(stored("13", "1,70", "1,82"), alt(0x46));
    }
    #[test]
    fn custom_shortcuts_roundtrip_and_invalid_shortcuts_are_rejected() {
        let mut settings = Settings::default();
        settings.presentation.shape_shortcuts = [
            Shortcut {
                modifiers: 6,
                key: 112,
            },
            Shortcut {
                modifiers: 2,
                key: 113,
            },
            Shortcut {
                modifiers: 9,
                key: 114,
            },
        ];
        assert_eq!(Settings::decode(&settings.encode()).unwrap(), settings);
        let valid = settings.encode();
        for invalid in [
            "16,70", "1,0", "1,255", "3,27", "2,113", "", "1", "1,2,3", "x,70", "1,-1",
        ] {
            let mut fields: Vec<_> = valid.lines().collect();
            fields[10] = invalid;
            assert!(
                Settings::decode(&(fields.join("\n") + "\n")).is_err(),
                "accepted {invalid:?}"
            );
        }
        settings.presentation.shape_shortcuts[2] = settings.presentation.shape_shortcuts[1];
        assert!(settings.presentation.validate_shape_shortcuts().is_err());
    }
    #[test]
    fn every_input_position_roundtrips_and_rejects_invalid_values() {
        for position in DisplayPosition::ALL {
            let mut settings = Settings::default();
            settings.presentation.input_position = position;
            assert_eq!(Settings::decode(&settings.encode()).unwrap(), settings);
        }
        let valid = Settings::default().encode();
        for invalid in ["", "6", "-1", "04", "+4", "bottom-center"] {
            let mut fields: Vec<_> = valid.lines().collect();
            fields[9] = invalid;
            assert!(Settings::decode(&(fields.join("\n") + "\n")).is_err());
        }
    }
    #[test]
    fn version_three_preserves_preferences_and_defaults_to_bottom_center() {
        let text = "shotoshot-settings-3\n3\n65\n1\n43,3a,2f,e9\n1\n5000\n010203\n040506\n";
        let settings = Settings::decode(text).unwrap();
        assert_eq!(
            settings.presentation.input_position,
            DisplayPosition::BottomCenter
        );
        assert_eq!(settings.presentation.duration_ms, 5000);
        assert_eq!(settings.presentation.halo_color, [1, 2, 3]);
        assert_eq!(settings.presentation.shape_color, [4, 5, 6]);
        assert_eq!(Settings::decode(&settings.encode()).unwrap(), settings);
        assert!(settings.live_capture);
        assert!(settings.legacy_style);
        assert_eq!(settings.modifiers, 3);
        assert_eq!(settings.key, 65);
        assert_eq!(settings.folder, PathBuf::from("C:/é"));
    }
    #[test]
    fn old_versions_upgrade_with_default_presentation_settings() {
        for (text, live_capture) in [
            ("shotoshot-settings-1\n3\n65\n1\n43,3a,2f,e9\n", "0"),
            ("shotoshot-settings-2\n3\n65\n1\n43,3a,2f,e9\n1\n", "1"),
        ] {
            let settings = Settings::decode(text).unwrap();
            assert_eq!(
                settings.encode(),
                format!(
                    "shotoshot-settings-13\n3\n65\n1\n43,3a,2f,e9\n{live_capture}\n3000\nffd340\ned4f5c\n4\n1,65\n1,82\n1,67\n5\n28\n-\n0\ned4f5c\n1\n3\n0,145\n1\n0\n100\n0\n"
                )
            );
        }
    }
    #[test]
    fn presentation_preferences_roundtrip_for_each_supported_duration() {
        for duration in [1000, 3000, 5000] {
            let text = format!(
                "shotoshot-settings-3\n3\n65\n0\n43,3a,2f,e9\n1\n{duration}\n00aAFF\nFf0001\n"
            );
            let settings = Settings::decode(&text).unwrap();
            assert_eq!(
                settings.encode(),
                format!(
                    "shotoshot-settings-13\n3\n65\n0\n43,3a,2f,e9\n1\n{duration}\n00aaff\nff0001\n4\n1,65\n1,82\n1,67\n5\n28\n-\n0\ned4f5c\n1\n3\n0,145\n1\n0\n100\n0\n"
                )
            );
        }
    }
    #[test]
    fn damaged_presentation_preferences_are_rejected() {
        let valid = "shotoshot-settings-3\n0\n19\n0\n\n1\n3000\nffd340\ned4f5c\n";
        assert!(Settings::decode(valid).is_ok());
        for (index, invalid_values) in [
            (0, vec!["shotoshot-settings-4"]),
            (5, vec!["2", "true", ""]),
            (
                6,
                vec!["0", "999", "2000", "5001", "-1000", "+1000", "01000", ""],
            ),
            (
                7,
                vec!["#ffd340", "ffd34", "ffd3400", "ggd340", "ffé40", ""],
            ),
            (
                8,
                vec!["#ed4f5c", "ed4f5", "ed4f5c0", "ed4f5g", " ed4f5c", ""],
            ),
        ] {
            for invalid in invalid_values {
                let mut fields: Vec<_> = valid.lines().collect();
                fields[index] = invalid;
                assert!(
                    Settings::decode(&(fields.join("\n") + "\n")).is_err(),
                    "accepted field {index}: {invalid:?}"
                );
            }
        }
        assert!(Settings::decode(&format!("{valid}1\n")).is_err());
        assert!(Settings::decode("shotoshot-settings-3\n0\n19\n0\n\n1\n3000\nffd340\n").is_err());
    }
    #[test]
    fn unicode_settings_roundtrip() {
        let s = Settings {
            instant_shortcut: Shortcut {
                modifiers: 0,
                key: 145,
            },
            instant_sound: true,
            start_with_windows: true,
            capture_stroke_width: 3,
            live_capture: true,
            modifiers: 3,
            key: 65,
            legacy_style: true,
            folder: PathBuf::from("C:/été/图像"),
            save_format: SaveFormat::Jpeg,
            last_custom_color: [237, 79, 92],
            presentation: PresentationSettings {
                stroke_width: 5,
                arrow_size: 28,
                shape_shortcuts: DEFAULT_SHAPE_SHORTCUTS,
                input_position: DisplayPosition::TopRight,
                input_offset: None,
                input_locked: false,
                input_layout: InputLayout::Gaming,
                input_scale: 150,
                duration_ms: 1000,
                halo_color: [0, 128, 255],
                shape_color: [4, 17, 92],
            },
        };
        assert_eq!(Settings::decode(&s.encode()).unwrap(), s);
    }
    #[test]
    fn rejects_future_and_invalid_shortcuts() {
        assert!(Settings::decode("shotoshot-settings-9\n0\n65\n0\n\n").is_err());
        assert!(Settings::decode("shotoshot-settings-1\n99\n65\n0\n\n").is_err());
    }
    #[test]
    fn old_settings_keep_frozen_default_and_new_mode_roundtrips() {
        let old = Settings::decode("shotoshot-settings-1\n0\n19\n1\n\n").unwrap();
        assert!(!old.live_capture);
        assert_eq!(old.key, 19);
        assert!(old.legacy_style);
        assert_eq!(Settings::decode(&old.encode()).unwrap(), old);
        assert!(Settings::decode("shotoshot-settings-2\n0\n19\n0\n\nx\n").is_err());
    }
    #[test]
    fn empty_folder_roundtrip() {
        assert_eq!(
            Settings::decode(&Settings::default().encode()).unwrap(),
            Settings::default()
        );
    }
}
