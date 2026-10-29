//! ROLE: The text Shotoshot adds to labwc's configuration while its magnifier is switched on:
//! a marked block in `rc.xml` (the magnifier's size and zoom, and the pointer button that
//! toggles it) and one in `themerc-override` (no border). labwc's own magnifier is drawn by the
//! compositor, so it never enlarges itself and recorders see it.
//! OWNER: The Linux resident writes the blocks when the magnifier is switched on and takes them
//! out when it is switched off, when it quits and at uninstall.
//! INVARIANTS: Adding then removing a block gives back the file byte for byte; the rest of the
//! file is never touched. A block is self-contained (it starts with its opening marker and ends
//! with the newline after its closing one), so removing it needs no guess about whitespace.

/// The pointer button labwc toggles the magnifier with: BTN_TASK, on no ordinary mouse. The
/// resident presses it through a virtual pointer, since labwc has no command for it.
pub const TOGGLE_BUTTON: u32 = 0x117;

const RC_BEGIN: &str = "<!-- shotoshot-magnifier begin";
const RC_END: &str = "<!-- shotoshot-magnifier end -->\n";
const THEME_BEGIN: &str = "# shotoshot-magnifier begin";
const THEME_END: &str = "# shotoshot-magnifier end\n";
/// Said by the opening marker when Shotoshot created the file, so removing the block may delete
/// the file it leaves as it was created.
const CREATED: &str = " (file created by Shotoshot)";

/// The `rc.xml` labwc reads when the user has none: an empty configuration when labwc merges
/// it over the system one (`labwc -m`), else the system file itself.
pub const RC_SKELETON: &str = "<?xml version=\"1.0\"?>\n<openbox_config xmlns=\"http://openbox.org/3.4/rc\">\n</openbox_config>\n";

/// The magnifier as the pointer settings choose it: a square of `size` logical pixels, `zoom`
/// times larger.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Magnifier {
    pub size: u32,
    pub zoom: u32,
}

/// The block of `rc.xml`: a round magnifier with a fading edge, as on Windows, under a labwc
/// that knows `<shape>` and `<feather>` (Shotoshot's patch, `docs/recherche/labwc-loupe-ronde/`);
/// any other labwc ignores them and draws its square. `mouse_defaults`: labwc loads its default mouse bindings only when
/// the configuration has none, so the block's single binding would otherwise remove them.
fn rc_block(magnifier: Magnifier, created: bool, mouse_defaults: bool) -> String {
    let created = if created { CREATED } else { "" };
    let defaults = if mouse_defaults {
        "  <default />\n"
    } else {
        ""
    };
    let Magnifier { size, zoom } = magnifier;
    // The Windows loupe's fading edge, in logical pixels here.
    let feather = crate::loupe::feather(size as i32);
    format!(
        "{RC_BEGIN}{created}: Shotoshot's magnifier, removed when it is switched off -->\n\
         <mouse>\n{defaults}  <context name=\"All\">\n    <mousebind button=\"Task\" action=\"Press\">\n      \
         <action name=\"ToggleMagnify\" />\n    </mousebind>\n  </context>\n</mouse>\n\
         <magnifier>\n  <width>{size}</width>\n  <height>{size}</height>\n  \
         <initScale>{zoom}</initScale>\n  <increment>1</increment>\n  <useFilter>no</useFilter>\n  \
         <shape>circle</shape>\n  <feather>{feather}</feather>\n</magnifier>\n{RC_END}"
    )
}

fn theme_block(created: bool) -> String {
    let created = if created { CREATED } else { "" };
    format!("{THEME_BEGIN}{created}\nmagnifier.border.width: 0\n{THEME_END}")
}

/// Where a block starts and ends in `text`, and whether Shotoshot created the file.
fn find(text: &str, begin: &str, end: &str) -> Option<(usize, usize, bool)> {
    let start = text.find(begin)?;
    let stop = start + text[start..].find(end)? + end.len();
    Some((
        start,
        stop,
        text[start..].starts_with(&format!("{begin}{CREATED}")),
    ))
}

/// `text` without its block, and whether Shotoshot created the file. Unchanged without a block.
fn take_out(text: &str, begin: &str, end: &str) -> (String, bool) {
    match find(text, begin, end) {
        Some((start, stop, created)) => (format!("{}{}", &text[..start], &text[stop..]), created),
        None => (text.to_owned(), false),
    }
}

/// What a file becomes once its block is removed: None to delete it, when Shotoshot created it
/// and nothing else changed since (`base` is what it was created from).
fn without(text: &str, begin: &str, end: &str, base: &str) -> Option<String> {
    let (rest, created) = take_out(text, begin, end);
    (!created || rest != base).then_some(rest)
}

/// `rc.xml` with the magnifier's block, replacing any former one. `text` is the user's file,
/// None when there is none; `base` is what a created file starts from (`RC_SKELETON` under
/// `labwc -m`, else the system file); `bindings` is the whole configuration labwc reads, to
/// know whether it already binds the mouse. None when the file has no `</openbox_config>`.
pub fn rc_with(
    text: Option<&str>,
    base: &str,
    bindings: &str,
    magnifier: Magnifier,
) -> Option<String> {
    let (rest, created) = match text {
        Some(text) => take_out(text, RC_BEGIN, RC_END),
        None => (base.to_owned(), true),
    };
    let close = rest.rfind("</openbox_config>")?;
    // At the start of the closing tag's line when only blanks precede it there.
    let line = rest[..close].rfind('\n').map_or(0, |i| i + 1);
    let at = if rest[line..close].trim().is_empty() {
        line
    } else {
        close
    };
    let defaults = !bindings.contains("<mousebind");
    let block = rc_block(magnifier, created, defaults);
    Some(format!("{}{block}{}", &rest[..at], &rest[at..]))
}

/// `rc.xml` without the magnifier's block; None to delete the file (see `without`).
pub fn rc_without(text: &str, base: &str) -> Option<String> {
    without(text, RC_BEGIN, RC_END, base)
}

/// `themerc-override` with the block that removes the magnifier's border, first in the file so
/// a border the user sets there still wins.
pub fn theme_with(text: Option<&str>) -> String {
    match text {
        Some(text) => {
            let (rest, created) = take_out(text, THEME_BEGIN, THEME_END);
            format!("{}{rest}", theme_block(created))
        }
        None => theme_block(true),
    }
}

/// `themerc-override` without the block; None to delete the file.
pub fn theme_without(text: &str) -> Option<String> {
    without(text, THEME_BEGIN, THEME_END, "")
}

/// Whether either file still holds a block.
pub fn has_block(text: &str) -> bool {
    text.contains(RC_BEGIN) || text.contains(THEME_BEGIN)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAGNIFIER: Magnifier = Magnifier { size: 240, zoom: 2 };

    /// The Raspberry Pi's own file, merged over the system one (`labwc -m`).
    const PI: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!-- Shotoshot: Print opens the capture editor, Shift+Print keeps the full-screen grim shot.\n     \
Merged over /etc/xdg/labwc/rc.xml (labwc -m). Delete this file to restore the defaults. -->\n\
<openbox_config xmlns=\"http://openbox.org/3.4/rc\">\n  <keyboard>\n    <keybind key=\"Print\">\n      \
<action name=\"Execute\">\n        <command>/home/chmyke/.local/bin/shotoshot --capture</command>\n      \
</action>\n    </keybind>\n  </keyboard>\n</openbox_config>\n";
    const SYSTEM_MOUSE: &str = "<mouse><default /><context name=\"Title\"><mousebind button=\"Left\" action=\"Drag\"/></context></mouse>";

    #[test]
    fn the_pi_file_comes_back_byte_for_byte() {
        let with = rc_with(Some(PI), RC_SKELETON, SYSTEM_MOUSE, MAGNIFIER).unwrap();
        assert!(with.contains("<width>240</width>") && with.contains("<initScale>2</initScale>"));
        assert!(with.contains("<action name=\"ToggleMagnify\" />"));
        // Round with the Windows loupe's fading edge, where labwc knows how.
        assert!(with.contains("<shape>circle</shape>"));
        let feather = crate::loupe::feather(240);
        assert!(with.contains(&format!("<feather>{feather}</feather>")));
        // The system file binds the mouse: its defaults stay without help.
        assert!(!with.contains("<default />"));
        assert!(with.ends_with(&format!("{RC_END}</openbox_config>\n")));
        assert!(with.starts_with(&PI[..PI.find("</openbox_config>").unwrap()]));
        assert_eq!(rc_without(&with, RC_SKELETON).as_deref(), Some(PI));
    }

    #[test]
    fn writing_again_replaces_the_block() {
        let first = rc_with(Some(PI), RC_SKELETON, SYSTEM_MOUSE, MAGNIFIER).unwrap();
        let again = rc_with(Some(&first), RC_SKELETON, SYSTEM_MOUSE, MAGNIFIER).unwrap();
        assert_eq!(again, first);
        let larger = Magnifier { size: 320, zoom: 3 };
        let changed = rc_with(Some(&first), RC_SKELETON, SYSTEM_MOUSE, larger).unwrap();
        assert_eq!(changed.matches(RC_BEGIN).count(), 1);
        assert!(
            changed.contains("<width>320</width>") && changed.contains("<initScale>3</initScale>")
        );
        assert_eq!(rc_without(&changed, RC_SKELETON).as_deref(), Some(PI));
    }

    #[test]
    fn a_missing_file_is_created_then_deleted() {
        let with = rc_with(None, RC_SKELETON, "", MAGNIFIER).unwrap();
        assert!(with.contains(CREATED));
        // Nothing binds the mouse: the block keeps labwc's default bindings.
        assert!(with.contains("<default />"));
        assert_eq!(rc_without(&with, RC_SKELETON), None);
        // Edited since: what remains is kept.
        let edited = with.replace("</openbox_config>", "<theme />\n</openbox_config>");
        assert_eq!(
            rc_without(&edited, RC_SKELETON).as_deref(),
            Some(
                RC_SKELETON
                    .replace("</openbox_config>", "<theme />\n</openbox_config>")
                    .as_str()
            )
        );
        // Created from the system file when labwc does not merge.
        let system = "<openbox_config>\n  <mouse><mousebind/></mouse>\n</openbox_config>\n";
        let copied = rc_with(None, system, system, MAGNIFIER).unwrap();
        assert!(!copied.contains("<default />"));
        assert_eq!(rc_without(&copied, system), None);
    }

    #[test]
    fn a_closing_tag_after_text_on_its_line_still_comes_back() {
        let text = "<openbox_config><keyboard /></openbox_config>";
        let with = rc_with(Some(text), RC_SKELETON, "", MAGNIFIER).unwrap();
        assert!(with.starts_with("<openbox_config><keyboard /><!-- shotoshot"));
        assert!(with.ends_with("-->\n</openbox_config>"));
        assert_eq!(rc_without(&with, RC_SKELETON).as_deref(), Some(text));
        // Indented closing tag, no final newline, Windows line ends.
        let text = "<openbox_config>\r\n  <keyboard />\r\n  </openbox_config>";
        let with = rc_with(Some(text), RC_SKELETON, "", MAGNIFIER).unwrap();
        assert!(with.contains("<keyboard />\r\n<!-- shotoshot"));
        assert_eq!(rc_without(&with, RC_SKELETON).as_deref(), Some(text));
    }

    #[test]
    fn a_file_without_its_root_is_left_alone() {
        assert_eq!(rc_with(Some(""), RC_SKELETON, "", MAGNIFIER), None);
        assert_eq!(
            rc_with(Some("<keyboard />\n"), RC_SKELETON, "", MAGNIFIER),
            None
        );
        assert_eq!(rc_without(PI, RC_SKELETON).as_deref(), Some(PI));
    }

    #[test]
    fn the_theme_override_loses_its_border_and_gets_it_back() {
        let created = theme_with(None);
        assert!(created.contains("magnifier.border.width: 0\n"));
        assert!(has_block(&created));
        assert_eq!(theme_without(&created), None);
        for text in [
            "",
            "window.active.title.bg.color: #000000\n",
            "border.width: 2",
        ] {
            let with = theme_with(Some(text));
            assert!(with.starts_with(THEME_BEGIN) && with.ends_with(text));
            assert_eq!(theme_with(Some(&with)), with);
            assert_eq!(theme_without(&with).as_deref(), Some(text));
        }
        assert!(!has_block(PI));
    }
}
