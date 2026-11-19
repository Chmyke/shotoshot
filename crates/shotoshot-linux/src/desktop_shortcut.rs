//! ROLE: Binds the capture shortcut in the desktop's own settings (labwc, GNOME, KDE, XFCE),
//! taking a bare Print Screen from its screenshot tool, and undoes it at uninstall.
//! OWNER: The shortcut window (`--setup`) applies; `--linux-shortcut-off` (uninstall) restores;
//! `--linux-shortcut-keep` (an update) records that the user binds the key himself.
//! INVARIANTS: `shortcut.txt` in the settings folder records what was set and the desktop value
//! it replaced; no record means the window has never been answered. Only the desktop value
//! Shotoshot changed is put back, and only while it is still what Shotoshot wrote.
use shotoshot_core::desktop_shortcut::{
    self as text, Desktop, GNOME_PATH, PRINT_SCREEN, QT_PRINT, Record,
};
use shotoshot_core::labwc_loupe::RC_SKELETON;
use std::{error::Error, fs, path::PathBuf, process::Command};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// The desktop of this session, when Shotoshot can set its shortcuts.
pub fn desktop() -> Option<Desktop> {
    Desktop::detect(
        &std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default(),
        std::env::var_os("LABWC_PID").is_some(),
    )
}

fn record_path() -> PathBuf {
    crate::settings::folder().join("shortcut.txt")
}
fn read_record() -> Result<Option<Record>> {
    match fs::read_to_string(record_path()) {
        Ok(text) => Ok(Some(Record::parse(&text).ok_or("damaged shortcut record")?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
fn write_record(record: &Record) -> Result<()> {
    let path = record_path();
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }
    Ok(fs::write(path, record.to_text())?)
}

/// Whether the first-install window still has to ask for the key.
pub fn needs_setup() -> bool {
    matches!(read_record(), Ok(None))
}

/// An update over a version that never asked: the key stays the user's business.
pub fn keep() -> Result<()> {
    if read_record()?.is_none() {
        write_record(&Record::default())?;
    }
    Ok(())
}

/// The command a shortcut runs: this program, captured.
pub fn command() -> Result<String> {
    let exe = std::env::current_exe()?;
    Ok(text::capture_command(&exe.to_string_lossy()))
}

/// Binds `mods`+`key` to the capture on `desktop` and records it; a former binding of
/// Shotoshot is undone first. On failure nothing stays changed.
pub fn apply(desktop: Desktop, mods: u32, key: u32) -> Result<()> {
    if let Some(former) = read_record()? {
        restore_record(&former)?;
    }
    let command = command()?;
    let native = match desktop {
        Desktop::Labwc => labwc::apply(mods, key, &command).map(|()| None),
        Desktop::Gnome => gnome::apply(mods, key, &command),
        Desktop::Kde => kde::apply(mods, key, &command),
        Desktop::Xfce => xfce::apply(mods, key, &command),
    }?;
    write_record(&Record {
        desktop: Some(desktop),
        shortcut: (mods, key),
        native,
    })
}

/// Undoes what Shotoshot set and forgets it: uninstalling. A later install asks again.
pub fn restore() -> Result<()> {
    if let Some(record) = read_record()? {
        restore_record(&record)?;
    }
    match fs::remove_file(record_path()) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}
fn restore_record(record: &Record) -> Result<()> {
    let (mods, key) = record.shortcut;
    let native = record.native.as_deref();
    match record.desktop {
        None => Ok(()),
        Some(Desktop::Labwc) => labwc::restore(),
        Some(Desktop::Gnome) => gnome::restore(native),
        Some(Desktop::Kde) => kde::restore(mods, key, native),
        Some(Desktop::Xfce) => xfce::restore(mods, key, native),
    }
}

/// Runs `program` and returns what it printed; its error output becomes the failure.
fn run(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    if !output.status.success() {
        let said = String::from_utf8_lossy(&output.stderr);
        return Err(format!("{program} {}: {}", args.join(" "), said.trim()).into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn is_bare_print(mods: u32, key: u32) -> bool {
    mods == 0 && key == PRINT_SCREEN
}
fn unknown_key() -> Box<dyn Error> {
    "this key has no name on Linux desktops".into()
}

mod labwc {
    use super::*;

    pub fn apply(mods: u32, key: u32, command: &str) -> Result<()> {
        let key = text::labwc_key(mods, key).ok_or_else(unknown_key)?;
        let files = crate::magnifier::Files::locate();
        let rc = files.read(&files.rc)?;
        let written = text::rc_with(
            rc.as_deref(),
            &files.base(),
            &files.bindings(rc.as_deref()),
            &key,
            command,
        )
        .ok_or("labwc's rc.xml has no </openbox_config>")?;
        crate::magnifier::write(&files.rc, &written)?;
        crate::magnifier::reconfigure()
    }
    pub fn restore() -> Result<()> {
        let files = crate::magnifier::Files::locate();
        let Some(rc) = files.read(&files.rc)?.filter(|t| text::rc_has_block(t)) else {
            return Ok(());
        };
        let rest =
            text::rc_without(&rc, RC_SKELETON).and_then(|_| text::rc_without(&rc, &files.system()));
        crate::magnifier::put(&files.rc, rest.as_deref())?;
        if crate::magnifier::available() {
            crate::magnifier::reconfigure()?;
        }
        Ok(())
    }
}

mod gnome {
    use super::*;
    const SHELL: &str = "org.gnome.shell.keybindings";
    const SCREENSHOT_UI: &str = "show-screenshot-ui";
    const MEDIA_KEYS: &str = "org.gnome.settings-daemon.plugins.media-keys";
    const LIST: &str = "custom-keybindings";

    fn ours() -> String {
        format!("{MEDIA_KEYS}.custom-keybinding:{GNOME_PATH}")
    }
    fn strv(schema: &str, key: &str) -> Result<Vec<String>> {
        let printed = run("gsettings", &["get", schema, key])?;
        text::strv_parse(&printed).ok_or_else(|| format!("{schema} {key}: {printed}").into())
    }

    /// Returns the screenshot keys gnome-shell had when Shotoshot took Print from them.
    pub fn apply(mods: u32, key: u32, command: &str) -> Result<Option<String>> {
        let binding = text::gtk_accelerator(mods, key).ok_or_else(unknown_key)?;
        let mut native = None;
        // gnome-shell must let Print go before gnome-settings-daemon can take it.
        if is_bare_print(mods, key) {
            let before = strv(SHELL, SCREENSHOT_UI)?;
            if before.iter().any(|b| b == "Print") {
                let after: Vec<String> = before.iter().filter(|b| *b != "Print").cloned().collect();
                run(
                    "gsettings",
                    &["set", SHELL, SCREENSHOT_UI, &text::strv_format(&after)],
                )?;
                native = Some(text::strv_format(&before));
            }
        }
        let added = (|| -> Result<()> {
            let mut list = strv(MEDIA_KEYS, LIST)?;
            if !list.iter().any(|p| p == GNOME_PATH) {
                list.push(GNOME_PATH.into());
                run(
                    "gsettings",
                    &["set", MEDIA_KEYS, LIST, &text::strv_format(&list)],
                )?;
            }
            let ours = ours();
            run("gsettings", &["set", &ours, "name", "'Shotoshot'"])?;
            run(
                "gsettings",
                &["set", &ours, "command", &text::gvariant_string(command)],
            )?;
            run(
                "gsettings",
                &["set", &ours, "binding", &text::gvariant_string(&binding)],
            )?;
            Ok(())
        })();
        if let Err(error) = added {
            let _ = restore(native.as_deref());
            return Err(error);
        }
        Ok(native)
    }
    pub fn restore(native: Option<&str>) -> Result<()> {
        let ours = ours();
        for key in ["binding", "command", "name"] {
            run("gsettings", &["reset", &ours, key])?;
        }
        let list = strv(MEDIA_KEYS, LIST)?;
        if list.iter().any(|p| p == GNOME_PATH) {
            let rest: Vec<String> = list.into_iter().filter(|p| p != GNOME_PATH).collect();
            run(
                "gsettings",
                &["set", MEDIA_KEYS, LIST, &text::strv_format(&rest)],
            )?;
        }
        // Print goes back to the screenshot tool unless the user has set its keys since.
        if let Some(native) = native.and_then(text::strv_parse) {
            let now = strv(SHELL, SCREENSHOT_UI)?;
            let taken: Vec<String> = native.iter().filter(|b| *b != "Print").cloned().collect();
            if now == taken {
                run(
                    "gsettings",
                    &["set", SHELL, SCREENSHOT_UI, &text::strv_format(&native)],
                )?;
            }
        }
        Ok(())
    }
}

mod xfce {
    use super::*;
    const CHANNEL: &str = "xfce4-keyboard-shortcuts";

    fn property(mods: u32, key: u32) -> Result<String> {
        let accel = text::gtk_accelerator(mods, key).ok_or_else(unknown_key)?;
        Ok(format!("/commands/custom/{accel}"))
    }
    fn read(property: &str) -> Option<String> {
        run("xfconf-query", &["-c", CHANNEL, "-p", property])
            .ok()
            .map(|v| v.trim_end_matches('\n').to_owned())
    }

    /// Returns the command the key ran before (xfce4-screenshooter for Print Screen).
    pub fn apply(mods: u32, key: u32, command: &str) -> Result<Option<String>> {
        // Without it XFCE reads its built-in shortcuts, and a custom tree holding only ours
        // would drop all the others.
        if read("/commands/custom/override").as_deref() != Some("true") {
            return Err("xfce4-keyboard-shortcuts has no custom shortcuts yet".into());
        }
        let property = property(mods, key)?;
        let native = read(&property);
        run(
            "xfconf-query",
            &[
                "-c", CHANNEL, "-p", &property, "-n", "-t", "string", "-s", command,
            ],
        )?;
        Ok(native)
    }
    pub fn restore(mods: u32, key: u32, native: Option<&str>) -> Result<()> {
        let property = property(mods, key)?;
        if !read(&property).is_some_and(|v| v.contains("shotoshot")) {
            return Ok(());
        }
        match native {
            Some(native) => run(
                "xfconf-query",
                &["-c", CHANNEL, "-p", &property, "-s", native],
            )
            .map(drop),
            None => run("xfconf-query", &["-c", CHANNEL, "-p", &property, "-r"]).map(drop),
        }
    }
}

mod kde {
    //! kglobalaccel over D-Bus: Spectacle's launch shortcut loses Print, and a desktop entry
    //! of Shotoshot's own, which kglobalaccel launches itself, gets the key.
    use super::*;
    use zbus::blocking::{Connection, Proxy};

    const COMPONENT: &str = "shotoshot-capture.desktop";
    const SPECTACLE: [&str; 4] = ["org.kde.spectacle.desktop", "_launch", "", ""];
    /// kglobalaccel's `NoAutoloading`: the keys given, not the ones it remembered.
    const NO_AUTOLOADING: u32 = 4;
    /// `QList<QKeySequence>` on D-Bus: each sequence is a struct of its keys, unused ones 0.
    type Keys = Vec<(Vec<i32>,)>;

    /// Whether `sequence` is the single key `key`, whatever padding Qt sends.
    fn is(sequence: &(Vec<i32>,), key: i32) -> bool {
        unpadded(&sequence.0) == [key]
    }
    fn unpadded(sequence: &[i32]) -> Vec<i32> {
        sequence.iter().copied().filter(|k| *k != 0).collect()
    }
    /// The sequences without padding, to compare what was read with what was recorded.
    fn plain(keys: &Keys) -> Vec<Vec<i32>> {
        keys.iter().map(|(k,)| unpadded(k)).collect()
    }

    fn accel(connection: &Connection) -> Result<Proxy<'_>> {
        Ok(Proxy::new(
            connection,
            "org.kde.kglobalaccel",
            "/kglobalaccel",
            "org.kde.KGlobalAccel",
        )?)
    }
    fn ours() -> [&'static str; 4] {
        [COMPONENT, "_launch", "Shotoshot", "Capture"]
    }
    fn entry() -> PathBuf {
        let data = std::env::var_os("XDG_DATA_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share")
            });
        data.join("kglobalaccel").join(COMPONENT)
    }
    fn to_text(keys: &Keys) -> String {
        keys.iter()
            .map(|(k,)| k.iter().map(i32::to_string).collect::<Vec<_>>().join("+"))
            .collect::<Vec<_>>()
            .join(" ")
    }
    fn from_text(text: &str) -> Keys {
        text.split_whitespace()
            .map(|seq| (seq.split('+').filter_map(|k| k.parse().ok()).collect(),))
            .collect()
    }

    /// Returns Spectacle's launch keys as they were when Print was taken from them.
    pub fn apply(mods: u32, key: u32, command: &str) -> Result<Option<String>> {
        let wanted = text::qt_key(mods, key).ok_or_else(unknown_key)?;
        let connection = Connection::session()?;
        let accel = accel(&connection)?;
        let mut native = None;
        if is_bare_print(mods, key) {
            let before: Keys = accel.call("shortcutKeys", &(SPECTACLE.to_vec(),))?;
            if before.iter().any(|k| is(k, QT_PRINT)) {
                let after: Keys = before
                    .iter()
                    .filter(|k| !is(k, QT_PRINT))
                    .cloned()
                    .collect();
                accel.call::<_, _, ()>("setForeignShortcutKeys", &(SPECTACLE.to_vec(), after))?;
                native = Some(to_text(&before));
            }
        }
        let path = entry();
        if let Some(folder) = path.parent() {
            fs::create_dir_all(folder)?;
        }
        fs::write(
            &path,
            format!(
                "[Desktop Entry]\nType=Application\nName=Shotoshot\nExec={command}\nIcon=shotoshot\nX-KDE-GlobalAccel-CommandShortcut=true\n"
            ),
        )?;
        let bound = (|| -> Result<()> {
            accel.call::<_, _, ()>("doRegister", &(ours().to_vec(),))?;
            let keys: Keys = vec![(vec![wanted],)];
            let set: Keys =
                accel.call("setShortcutKeys", &(ours().to_vec(), keys, NO_AUTOLOADING))?;
            if !set.iter().any(|k| is(k, wanted)) {
                return Err("kglobalaccel kept the key for another action".into());
            }
            Ok(())
        })();
        if let Err(error) = bound {
            let _ = restore(mods, key, native.as_deref());
            return Err(error);
        }
        Ok(native)
    }
    pub fn restore(_: u32, _: u32, native: Option<&str>) -> Result<()> {
        let connection = Connection::session()?;
        let accel = accel(&connection)?;
        let _: bool = accel.call("unregister", &(COMPONENT, "_launch"))?;
        match fs::remove_file(entry()) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
        if let Some(native) = native {
            let now: Keys = accel.call("shortcutKeys", &(SPECTACLE.to_vec(),))?;
            let before = from_text(native);
            let taken: Keys = before
                .iter()
                .filter(|k| !is(k, QT_PRINT))
                .cloned()
                .collect();
            if plain(&now) == plain(&taken) {
                accel.call::<_, _, ()>("setForeignShortcutKeys", &(SPECTACLE.to_vec(), before))?;
            }
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn spectacle_keys_survive_the_record() {
            let keys: Keys = vec![(vec![QT_PRINT],), (vec![0x1200_0053, 0x41],)];
            assert_eq!(from_text(&to_text(&keys)), keys);
        }
    }
}

/// `--linux-setup-probe`: binds Print Screen on this desktop as the window's Save does, checks
/// the desktop holds it, then puts everything back and checks nothing is left. The record and
/// the settings are never touched.
pub fn probe() -> Result<()> {
    let desktop = desktop().ok_or("setup probe: no desktop Shotoshot sets")?;
    if desktop != Desktop::Labwc {
        return Err(format!("setup probe: {} is not checked here", desktop.tag()).into());
    }
    let files = crate::magnifier::Files::locate();
    let before = files.read(&files.rc)?;
    let command = command()?;
    let applied = labwc::apply(0, PRINT_SCREEN, &command);
    let written = files.read(&files.rc)?;
    let restored = labwc::restore();
    let after = files.read(&files.rc)?;
    applied?;
    restored?;
    let written = written.unwrap_or_default();
    if !written.contains("<keybind key=\"Print\">") || !written.contains(&command) {
        return Err(format!("setup probe: rc.xml without the keybind:\n{written}").into());
    }
    if after != before {
        return Err("setup probe: rc.xml not given back byte for byte".into());
    }
    println!("setup: Print bound in labwc's rc.xml, then given back byte for byte");
    Ok(())
}
