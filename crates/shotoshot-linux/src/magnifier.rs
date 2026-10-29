//! ROLE: The live magnifier under labwc: labwc's own (`ToggleMagnify`), drawn by the compositor
//! around the pointer, so it never enlarges itself and recorders see it. A magnifier drawn by an
//! application would capture itself (docs/recherche/2026-10-28-loupe-linux.md).
//! OWNER: The resident's menu switches it; `--linux-magnifier-off` (uninstall) forgets it.
//! INVARIANTS: Shotoshot's blocks are in labwc's files only while the magnifier is on; the rest
//! of those files is never touched (`shotoshot_core::labwc_loupe`). labwc has no command for
//! the magnifier: Shotoshot presses the button its block binds, through a virtual pointer
//! opened only for that press.
use shotoshot_core::labwc_loupe::{self, Magnifier, RC_SKELETON, TOGGLE_BUTTON};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Whether this session runs labwc, the only desktop whose magnifier Shotoshot drives.
pub fn available() -> bool {
    std::env::var_os("LABWC_PID").is_some()
}

/// The magnifier as the pointer settings choose it (shared with Windows).
pub fn chosen(settings: &shotoshot_core::settings::Settings) -> Magnifier {
    let pointer = &settings.presentation.pointer;
    Magnifier {
        size: pointer.loupe_size,
        zoom: pointer.loupe_zoom,
    }
}

/// Writes the blocks, has labwc read them, then switches its magnifier on.
pub fn switch_on(magnifier: Magnifier) -> Result<()> {
    let files = Files::locate();
    let rc = files.read(&files.rc)?;
    let text = labwc_loupe::rc_with(
        rc.as_deref(),
        &files.base(),
        &files.bindings(rc.as_deref()),
        magnifier,
    )
    .ok_or("labwc's rc.xml has no </openbox_config>")?;
    let theme = labwc_loupe::theme_with(files.read(&files.theme)?.as_deref());
    write(&files.rc, &text)?;
    write(&files.theme, &theme)?;
    reconfigure()?;
    press_toggle(true)
}

/// Switches labwc's magnifier off, then takes the blocks out.
pub fn switch_off() -> Result<()> {
    let pressed = press_toggle(false);
    forget()?;
    pressed
}

/// Takes the blocks out and has labwc read its files again, magnifier or not: what quitting,
/// uninstalling and a resident starting after a crash do.
pub fn forget() -> Result<()> {
    let files = Files::locate();
    let mut changed = false;
    if let Some(text) = files.read(&files.rc)?.filter(|t| labwc_loupe::has_block(t)) {
        let rest = labwc_loupe::rc_without(&text, RC_SKELETON)
            .and_then(|_| labwc_loupe::rc_without(&text, &files.system()));
        put(&files.rc, rest.as_deref())?;
        changed = true;
    }
    if let Some(text) = files
        .read(&files.theme)?
        .filter(|t| labwc_loupe::has_block(t))
    {
        put(&files.theme, labwc_loupe::theme_without(&text).as_deref())?;
        changed = true;
    }
    if changed && available() {
        reconfigure()?;
    }
    Ok(())
}

/// `--linux-magnifier-probe`: on for a second at the default size, then off, the blocks gone.
pub fn probe() -> Result<()> {
    if !available() {
        return Err("magnifier: LABWC_PID is not set".into());
    }
    let pointer = shotoshot_core::pointer::PointerSettings::default();
    let magnifier = Magnifier {
        size: pointer.loupe_size,
        zoom: pointer.loupe_zoom,
    };
    let on = switch_on(magnifier);
    let files = Files::locate();
    let written = [&files.rc, &files.theme].map(|path| {
        files
            .read(path)
            .ok()
            .flatten()
            .is_some_and(|t| labwc_loupe::has_block(&t))
    });
    std::thread::sleep(std::time::Duration::from_secs(1));
    let off = if on.is_ok() { switch_off() } else { forget() };
    on?;
    off?;
    if written != [true, true] {
        return Err(format!("magnifier: blocks written {written:?}").into());
    }
    for path in [&files.rc, &files.theme] {
        if files
            .read(path)?
            .is_some_and(|t| labwc_loupe::has_block(&t))
        {
            return Err(format!("magnifier: block left in {}", path.display()).into());
        }
    }
    println!("magnifier: switched on and off, blocks removed");
    Ok(())
}

struct Files {
    rc: PathBuf,
    theme: PathBuf,
    /// The system configuration folders, most important first.
    system: Vec<PathBuf>,
    /// Whether labwc merges the user's rc.xml over the system ones (`labwc -m`).
    merge: bool,
}
impl Files {
    fn locate() -> Self {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
            });
        let dirs = std::env::var("XDG_CONFIG_DIRS")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "/etc/xdg".into());
        let merge = std::env::var("LABWC_PID")
            .ok()
            .and_then(|pid| fs::read(format!("/proc/{pid}/cmdline")).ok())
            .is_some_and(|line| {
                line.split(|&b| b == 0)
                    .any(|arg| arg == b"-m" || arg == b"--merge-config")
            });
        Self {
            rc: config.join("labwc/rc.xml"),
            theme: config.join("labwc/themerc-override"),
            system: dirs
                .split(':')
                .map(|d| Path::new(d).join("labwc/rc.xml"))
                .collect(),
            merge,
        }
    }
    fn read(&self, path: &Path) -> Result<Option<String>> {
        match fs::read_to_string(path) {
            Ok(text) => Ok(Some(text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("{}: {e}", path.display()).into()),
        }
    }
    /// The system rc.xml labwc reads when the user has none.
    fn system(&self) -> String {
        self.system
            .iter()
            .find_map(|path| fs::read_to_string(path).ok())
            .unwrap_or_default()
    }
    /// What a created rc.xml starts from.
    fn base(&self) -> String {
        match self.system() {
            system if !self.merge && !system.is_empty() => system,
            _ => RC_SKELETON.into(),
        }
    }
    /// The whole configuration labwc reads, to know whether it binds the mouse.
    fn bindings(&self, user: Option<&str>) -> String {
        let system = || -> String {
            if self.merge {
                self.system
                    .iter()
                    .filter_map(|p| fs::read_to_string(p).ok())
                    .collect()
            } else {
                self.system()
            }
        };
        match user {
            Some(user) if self.merge => system() + user,
            Some(user) => user.into(),
            None => system(),
        }
    }
}

/// Writes `text` in place of `path` at once (a reader never sees half a file), keeping its
/// permissions; None deletes it.
fn put(path: &Path, text: Option<&str>) -> Result<()> {
    match text {
        Some(text) => write(path, text),
        None => Ok(fs::remove_file(path)?),
    }
}
fn write(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("shotoshot-new");
    fs::write(&temporary, text)?;
    if let Ok(metadata) = fs::metadata(path) {
        fs::set_permissions(&temporary, metadata.permissions())?;
    }
    Ok(fs::rename(&temporary, path)?)
}

/// Has labwc read its files again (SIGHUP to `LABWC_PID`).
fn reconfigure() -> Result<()> {
    let status = Command::new("labwc").arg("--reconfigure").status()?;
    if !status.success() {
        return Err(format!("labwc --reconfigure: {status}").into());
    }
    Ok(())
}

/// Presses then releases the button labwc's block binds to `ToggleMagnify`. `after_reload`:
/// labwc reloads on its signal in its own loop, so two round trips first make sure it has read
/// the new binding before the press.
fn press_toggle(after_reload: bool) -> Result<()> {
    use wayland_client::{
        Connection, Dispatch, QueueHandle, delegate_noop,
        globals::{GlobalListContents, registry_queue_init},
        protocol::{wl_pointer::ButtonState, wl_registry, wl_seat},
    };
    use wayland_protocols_wlr::virtual_pointer::v1::client::{
        zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1 as Manager,
        zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1 as Pointer,
    };
    struct State;
    impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
        fn event(
            _: &mut Self,
            _: &wl_registry::WlRegistry,
            _: wl_registry::Event,
            _: &GlobalListContents,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }
    delegate_noop!(State: ignore wl_seat::WlSeat);
    delegate_noop!(State: Manager);
    delegate_noop!(State: Pointer);

    let connection = Connection::connect_to_env()?;
    let (globals, mut queue) = registry_queue_init::<State>(&connection)?;
    let handle = queue.handle();
    let seat: wl_seat::WlSeat = globals.bind(&handle, 1..=1, ())?;
    let manager: Manager = globals.bind(&handle, 1..=1, ())?;
    if after_reload {
        queue.roundtrip(&mut State)?;
        queue.roundtrip(&mut State)?;
    }
    let pointer = manager.create_virtual_pointer(Some(&seat), &handle, ());
    for (time, state) in [(0, ButtonState::Pressed), (1, ButtonState::Released)] {
        pointer.button(time, TOGGLE_BUTTON, state);
        pointer.frame();
    }
    pointer.destroy();
    manager.destroy();
    queue.roundtrip(&mut State)?;
    Ok(())
}
