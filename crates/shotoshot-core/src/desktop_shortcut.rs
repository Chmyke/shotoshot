//! ROLE: What Shotoshot writes into a Linux desktop's own shortcuts so that a key runs
//! `shotoshot --capture`: the key names each desktop reads, the block added to labwc's
//! `rc.xml`, GSettings lists, and the record kept to undo it all.
//! OWNER: The Linux shortcut window applies it once, at the first install; uninstalling undoes
//! it. Pure text, so it is tested on every system.
//! INVARIANTS: Only bare Print Screen is taken from the desktop's own screenshot tool; its other
//! shortcuts stay. Adding then removing the labwc block gives back the file byte for byte.
//! Sources checked on 2026-11-05: docs/contexte/portage.md, « Touche de capture ».
use crate::labwc_loupe::{CREATED, take_out, without};

/// Windows virtual-key code of Print Screen, the key every system proposes first.
pub const PRINT_SCREEN: u32 = 0x2C;
/// Modifier bits as Windows numbers them, the settings format on every system.
pub const ALT: u32 = 1;
pub const CTRL: u32 = 2;
pub const SHIFT: u32 = 4;
pub const LOGO: u32 = 8;

/// The desktops whose shortcuts Shotoshot sets itself.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Desktop {
    /// Raspberry Pi OS since 2024: `~/.config/labwc/rc.xml`.
    Labwc,
    /// gnome-shell holds Print; custom shortcuts belong to gnome-settings-daemon.
    Gnome,
    /// kglobalaccel, over D-Bus (Spectacle holds Print).
    Kde,
    /// xfconf, channel `xfce4-keyboard-shortcuts` (xfce4-screenshooter holds Print).
    Xfce,
}
impl Desktop {
    /// The desktop of a session whose `XDG_CURRENT_DESKTOP` is `current`, labwc being also
    /// known by the `LABWC_PID` it sets. GNOME derivatives with shortcuts of their own (Budgie,
    /// Cinnamon, Unity, Pantheon) are left out.
    pub fn detect(current: &str, labwc_pid: bool) -> Option<Self> {
        if labwc_pid {
            return Some(Self::Labwc);
        }
        let parts: Vec<String> = current.split(':').map(str::to_ascii_lowercase).collect();
        let has = |name: &str| parts.iter().any(|part| part == name);
        if has("labwc") {
            Some(Self::Labwc)
        } else if has("kde") {
            Some(Self::Kde)
        } else if has("xfce") {
            Some(Self::Xfce)
        } else if has("gnome")
            && !["budgie", "x-cinnamon", "unity", "pantheon", "mate"]
                .iter()
                .any(|other| has(other))
        {
            Some(Self::Gnome)
        } else {
            None
        }
    }
    pub fn tag(self) -> &'static str {
        match self {
            Self::Labwc => "labwc",
            Self::Gnome => "gnome",
            Self::Kde => "kde",
            Self::Xfce => "xfce",
        }
    }
    fn from_tag(tag: &str) -> Option<Self> {
        [Self::Labwc, Self::Gnome, Self::Kde, Self::Xfce]
            .into_iter()
            .find(|desktop| desktop.tag() == tag)
    }
}

/// The X keysym name of virtual key `key`, for the keys a capture shortcut may use; None for
/// the others, which the Linux window does not record.
pub fn keysym(key: u32) -> Option<String> {
    Some(match key {
        // Letters by their lower-case keysym, as the desktops write them.
        65..=90 => char::from(key as u8 + 32).to_string(),
        48..=57 => char::from(key as u8).to_string(),
        112..=135 => format!("F{}", key - 111),
        _ => match key {
            44 => "Print",
            19 => "Pause",
            145 => "Scroll_Lock",
            45 => "Insert",
            46 => "Delete",
            36 => "Home",
            35 => "End",
            33 => "Prior",
            34 => "Next",
            37 => "Left",
            38 => "Up",
            39 => "Right",
            40 => "Down",
            8 => "BackSpace",
            9 => "Tab",
            13 => "Return",
            32 => "space",
            _ => return None,
        }
        .to_owned(),
    })
}

/// The accelerator GTK desktops (GNOME, XFCE) read: `<Primary><Shift>Print`.
pub fn gtk_accelerator(mods: u32, key: u32) -> Option<String> {
    let mut text = String::new();
    for (mask, name) in [
        (CTRL, "<Primary>"),
        (SHIFT, "<Shift>"),
        (ALT, "<Alt>"),
        (LOGO, "<Super>"),
    ] {
        if mods & mask != 0 {
            text.push_str(name);
        }
    }
    Some(text + &keysym(key)?)
}

/// The key labwc's `<keybind key="…">` reads: `C-S-Print`.
pub fn labwc_key(mods: u32, key: u32) -> Option<String> {
    let mut text = String::new();
    for (mask, name) in [(LOGO, "W-"), (CTRL, "C-"), (ALT, "A-"), (SHIFT, "S-")] {
        if mods & mask != 0 {
            text.push_str(name);
        }
    }
    Some(text + &keysym(key)?)
}

/// The key as Qt combines it (`Qt::Key` plus `Qt::KeyboardModifier` bits), the integer
/// kglobalaccel exchanges over D-Bus.
pub fn qt_key(mods: u32, key: u32) -> Option<i32> {
    let base: i32 = match key {
        65..=90 | 48..=57 | 32 => key as i32,
        112..=135 => 0x0100_0030 + (key as i32 - 112),
        44 => 0x0100_0009,
        19 => 0x0100_0008,
        145 => 0x0100_0026,
        45 => 0x0100_0006,
        46 => 0x0100_0007,
        36 => 0x0100_0010,
        35 => 0x0100_0011,
        33 => 0x0100_0016,
        34 => 0x0100_0017,
        37 => 0x0100_0012,
        38 => 0x0100_0013,
        39 => 0x0100_0014,
        40 => 0x0100_0015,
        8 => 0x0100_0003,
        9 => 0x0100_0001,
        13 => 0x0100_0004,
        _ => return None,
    };
    let mut combined = base;
    for (mask, bit) in [
        (SHIFT, 0x0200_0000),
        (CTRL, 0x0400_0000),
        (ALT, 0x0800_0000),
        (LOGO, 0x1000_0000),
    ] {
        if mods & mask != 0 {
            combined |= bit;
        }
    }
    Some(combined)
}
/// Qt's Print Screen alone.
pub const QT_PRINT: i32 = 0x0100_0009;

/// `path` as one argument of a command line that desktops split the way a shell does
/// (`g_shell_parse_argv`, labwc and GNOME alike).
pub fn shell_quoted(path: &str) -> String {
    format!("'{}'", path.replace('\'', "'\\''"))
}

/// The command a shortcut runs.
pub fn capture_command(exe: &str) -> String {
    format!("{} --capture", shell_quoted(exe))
}

const RC_BEGIN: &str = "<!-- shotoshot-capture begin";
const RC_END: &str = "<!-- shotoshot-capture end -->\n";

fn xml_escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The actions of the last `<keybind key="{key}">` of `config`, as written there.
fn keybind_actions<'t>(config: &'t str, key: &str) -> Option<&'t str> {
    let open = format!("<keybind key=\"{key}\">");
    let start = config.rfind(&open)? + open.len();
    let stop = start + config[start..].find("</keybind>")?;
    Some(&config[start..stop])
}

/// `rc.xml` with Shotoshot's keybind for `key` running `command`, replacing a former one.
/// `text` is the user's file, None when there is none; `base` is what a created file starts
/// from (`RC_SKELETON` under `labwc -m`, else the system file); `bindings` is the whole
/// configuration labwc reads. A bare Print Screen keeps the desktop's own Print Screen action
/// on Shift+Print when that key is free. None when the file has no `</openbox_config>`.
pub fn rc_with(
    text: Option<&str>,
    base: &str,
    bindings: &str,
    key: &str,
    command: &str,
) -> Option<String> {
    let (rest, created) = match text {
        Some(text) => take_out(text, RC_BEGIN, RC_END),
        None => (base.to_owned(), true),
    };
    let (outside, _) = take_out(bindings, RC_BEGIN, RC_END);
    let close = rest.rfind("</openbox_config>")?;
    let line = rest[..close].rfind('\n').map_or(0, |i| i + 1);
    let at = if rest[line..close].trim().is_empty() {
        line
    } else {
        close
    };
    let created = if created { CREATED } else { "" };
    // labwc loads its default keybinds only when the configuration has none.
    let defaults = if outside.contains("<keybind") {
        ""
    } else {
        "  <default />\n"
    };
    let mut binds = format!(
        "  <keybind key=\"{key}\">\n    <action name=\"Execute\">\n      <command>{}</command>\n    </action>\n  </keybind>\n",
        xml_escaped(command)
    );
    if key == "Print"
        && !outside.contains("<keybind key=\"S-Print\">")
        && let Some(native) =
            keybind_actions(&outside, "Print").filter(|a| !a.contains("shotoshot"))
    {
        binds.push_str(&format!("  <keybind key=\"S-Print\">{native}</keybind>\n"));
    }
    let block = format!(
        "{RC_BEGIN}{created}: Shotoshot's capture shortcut, removed at uninstall -->\n<keyboard>\n{defaults}{binds}</keyboard>\n{RC_END}"
    );
    Some(format!("{}{block}{}", &rest[..at], &rest[at..]))
}

/// `rc.xml` without Shotoshot's keybind; None to delete the file Shotoshot created.
pub fn rc_without(text: &str, base: &str) -> Option<String> {
    without(text, RC_BEGIN, RC_END, base)
}

/// Whether `text` holds Shotoshot's keybind.
pub fn rc_has_block(text: &str) -> bool {
    text.contains(RC_BEGIN)
}

/// The strings of a GVariant string array as `gsettings get` prints it (`['a', 'b']`, `@as []`).
pub fn strv_parse(text: &str) -> Option<Vec<String>> {
    let inner = text
        .trim()
        .trim_start_matches("@as")
        .trim()
        .strip_prefix('[')?
        .strip_suffix(']')?;
    let mut items = Vec::new();
    let mut chars = inner.chars().peekable();
    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace() || *c == ',') {
            chars.next();
        }
        let Some(quote) = chars.next() else {
            return Some(items);
        };
        if quote != '\'' && quote != '"' {
            return None;
        }
        let mut item = String::new();
        loop {
            match chars.next()? {
                '\\' => item.push(chars.next()?),
                c if c == quote => break,
                c => item.push(c),
            }
        }
        items.push(item);
    }
}

/// One GVariant string, as `gsettings set` reads it.
pub fn gvariant_string(text: &str) -> String {
    format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// A GVariant string array, as `gsettings set` reads it.
pub fn strv_format(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|item| gvariant_string(item)).collect();
    format!("[{}]", quoted.join(", "))
}

/// The GSettings path of Shotoshot's GNOME custom shortcut.
pub const GNOME_PATH: &str =
    "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/shotoshot/";

/// What Shotoshot set, to undo it at uninstall. `desktop` None: the user binds the key
/// himself (an update of a version that never asked, or a desktop Shotoshot cannot set).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Record {
    pub desktop: Option<Desktop>,
    /// The modifiers and key Shotoshot bound.
    pub shortcut: (u32, u32),
    /// The desktop's own value Shotoshot changed to free Print Screen, as read then.
    pub native: Option<String>,
}
const RECORD_HEADER: &str = "shotoshot-desktop-shortcut-1";

impl Record {
    pub fn to_text(&self) -> String {
        let mut text = format!(
            "{RECORD_HEADER}\ndesktop {}\n",
            self.desktop.map_or("none", Desktop::tag)
        );
        if self.desktop.is_some() {
            text += &format!("shortcut {},{}\n", self.shortcut.0, self.shortcut.1);
        }
        if let Some(native) = &self.native {
            text += &format!("native {}\n", native.replace('\n', " "));
        }
        text
    }
    pub fn parse(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        (lines.next()? == RECORD_HEADER).then_some(())?;
        let mut record = Self::default();
        for line in lines {
            let (name, value) = line.split_once(' ').unwrap_or((line, ""));
            match name {
                "desktop" if value == "none" => record.desktop = None,
                "desktop" => record.desktop = Some(Desktop::from_tag(value)?),
                "shortcut" => {
                    let (mods, key) = value.split_once(',')?;
                    record.shortcut = (mods.parse().ok()?, key.parse().ok()?);
                }
                "native" => record.native = Some(value.to_owned()),
                _ => return None,
            }
        }
        Some(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::labwc_loupe::RC_SKELETON;

    #[test]
    fn desktops_are_told_apart_by_their_session_names() {
        assert_eq!(Desktop::detect("", true), Some(Desktop::Labwc));
        assert_eq!(
            Desktop::detect("labwc:wlroots", false),
            Some(Desktop::Labwc)
        );
        assert_eq!(Desktop::detect("ubuntu:GNOME", false), Some(Desktop::Gnome));
        assert_eq!(Desktop::detect("GNOME", false), Some(Desktop::Gnome));
        assert_eq!(Desktop::detect("KDE", false), Some(Desktop::Kde));
        assert_eq!(Desktop::detect("XFCE", false), Some(Desktop::Xfce));
        for other in ["Budgie:GNOME", "X-Cinnamon", "MATE", "Hyprland", "sway", ""] {
            assert_eq!(Desktop::detect(other, false), None, "{other}");
        }
    }

    #[test]
    fn each_desktop_reads_the_key_in_its_own_words() {
        assert_eq!(gtk_accelerator(0, PRINT_SCREEN).unwrap(), "Print");
        assert_eq!(
            gtk_accelerator(CTRL | SHIFT, 0x41).unwrap(),
            "<Primary><Shift>a"
        );
        assert_eq!(labwc_key(CTRL | ALT, 112).unwrap(), "C-A-F1");
        assert_eq!(labwc_key(SHIFT, PRINT_SCREEN).unwrap(), "S-Print");
        assert_eq!(qt_key(0, PRINT_SCREEN), Some(QT_PRINT));
        assert_eq!(qt_key(SHIFT, PRINT_SCREEN), Some(0x0200_0009 | QT_PRINT));
        assert_eq!(qt_key(CTRL, 0x58), Some(0x0400_0058));
        // A key no desktop name is known for is never recorded.
        for key in [0x5B, 96, 0xBA] {
            assert_eq!(keysym(key), None);
            assert_eq!(qt_key(0, key), None);
        }
    }

    #[test]
    fn a_path_with_quotes_stays_one_argument() {
        assert_eq!(
            capture_command("/home/a b/it's/shotoshot"),
            "'/home/a b/it'\\''s/shotoshot' --capture"
        );
    }

    const PI_SYSTEM: &str = "<?xml version=\"1.0\"?>\n<openbox_config>\n<keyboard>\n    <keybind key=\"Print\">\n      <action name=\"Execute\">\n        <command>grim</command>\n      </action>\n    </keybind>\n</keyboard>\n</openbox_config>\n";

    #[test]
    fn labwc_takes_print_screen_and_keeps_its_tool_on_shift() {
        let user = "<?xml version=\"1.0\"?>\n<openbox_config>\n  <theme />\n</openbox_config>\n";
        let bindings = format!("{PI_SYSTEM}{user}");
        let with = rc_with(
            Some(user),
            RC_SKELETON,
            &bindings,
            "Print",
            "'/x' --capture",
        )
        .unwrap();
        assert!(with.contains("<keybind key=\"Print\">"));
        assert!(with.contains("<command>'/x' --capture</command>"));
        let shifted = keybind_actions(&with, "S-Print").unwrap();
        assert!(shifted.contains("<command>grim</command>"), "{with}");
        assert!(
            !with.contains("<default />"),
            "the system file has keybinds"
        );
        assert!(with.ends_with("</openbox_config>\n"));
        // Undone byte for byte, and applying twice keeps one block.
        assert_eq!(rc_without(&with, RC_SKELETON).unwrap(), user);
        let twice = rc_with(
            Some(&with),
            RC_SKELETON,
            &bindings,
            "Print",
            "'/x' --capture",
        )
        .unwrap();
        assert_eq!(twice, with);
    }

    #[test]
    fn labwc_leaves_shift_print_alone_when_it_is_taken_or_the_key_is_another() {
        let taken = format!(
            "{PI_SYSTEM}<keybind key=\"S-Print\"><action name=\"Execute\" command=\"x\"/></keybind>"
        );
        let with = rc_with(None, RC_SKELETON, &taken, "Print", "c").unwrap();
        assert!(!with.contains("S-Print"));
        let other = rc_with(None, RC_SKELETON, PI_SYSTEM, "C-F1", "c").unwrap();
        assert!(!other.contains("S-Print"));
        // A file Shotoshot created is deleted again.
        assert_eq!(rc_without(&other, RC_SKELETON), None);
    }

    #[test]
    fn labwc_without_any_keybind_keeps_its_defaults() {
        let with = rc_with(None, RC_SKELETON, RC_SKELETON, "Print", "c").unwrap();
        assert!(with.contains("<keyboard>\n  <default />\n"));
        assert!(!with.contains("S-Print"));
    }

    #[test]
    fn gsettings_lists_read_and_write_back() {
        assert_eq!(strv_parse("@as []").unwrap(), Vec::<String>::new());
        assert_eq!(
            strv_parse("['Print', '<Super>p']\n").unwrap(),
            ["Print", "<Super>p"]
        );
        assert_eq!(strv_parse("['it\\'s']").unwrap(), ["it's"]);
        assert_eq!(strv_parse("nonsense"), None);
        let items = vec!["/a/".to_owned(), "it's".to_owned()];
        assert_eq!(strv_format(&items), "['/a/', 'it\\'s']");
        assert_eq!(strv_parse(&strv_format(&items)).unwrap(), items);
    }

    #[test]
    fn the_record_survives_a_round_trip() {
        for record in [
            Record::default(),
            Record {
                desktop: Some(Desktop::Gnome),
                shortcut: (0, PRINT_SCREEN),
                native: Some("['Print']".into()),
            },
            Record {
                desktop: Some(Desktop::Labwc),
                shortcut: (CTRL, 0x41),
                native: None,
            },
        ] {
            assert_eq!(Record::parse(&record.to_text()).unwrap(), record);
        }
        assert_eq!(Record::parse("damaged"), None);
    }
}
