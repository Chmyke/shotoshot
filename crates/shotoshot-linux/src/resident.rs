//! ROLE: Linux resident: notification-area icon and its menu. Each capture runs in its own
//! short-lived process (`--capture`, `--instant`), so the resident stays small at rest.
//! INVARIANTS: One resident per user session (lock in XDG_RUNTIME_DIR).
use crate::update::Checked;
use shotoshot_core::update::Outcome;
use shotoshot_text::{Key, Language, LanguageChoice, Text, language, tr_in};
use shotoshot_ui::settings_panel::{UpdateState, running_version};
use std::{
    error::Error,
    process::Command,
    sync::mpsc::{Sender, channel},
};

enum Request {
    Capture,
    Instant,
    Language(LanguageChoice),
    Magnifier,
    /// The menu's "Check for updates": the one request to the site, on this click only.
    CheckUpdates,
    /// What that request found, from its thread.
    Checked(Checked),
    OpenDownloadPage,
    Quit,
}

struct Tray {
    requests: Sender<Request>,
    icons: Vec<ksni::Icon>,
    /// Whether labwc's magnifier is on; None outside labwc, where the menu has no such line.
    magnifier: Option<bool>,
    /// Where "Check for updates" stands: its line says it.
    update: UpdateState,
}
impl Tray {
    fn send(&self, request: Request) {
        let _ = self.requests.send(request);
    }
}
impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "shotoshot".into()
    }
    fn title(&self) -> String {
        "Shotoshot".into()
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.icons.clone()
    }
    fn activate(&mut self, _: i32, _: i32) {
        self.send(Request::Capture);
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        menu_in(language(), self.magnifier, self.update)
    }
}

/// A text as a menu label: an underscore would otherwise mark an access key.
fn label(text: &str) -> String {
    text.replace('_', "__")
}

/// The menu in `language`, that language marked in the language submenu, and the magnifier's
/// line when `magnifier` says whether it is on. Linux has no Settings window: the language
/// stays here, without a "system language" row (Mickael, 2026-11-12), and so does "Check for
/// updates", whose line becomes "Download version X" once a newer one was found (2026-11-13).
fn menu_in(
    language: Language,
    magnifier: Option<bool>,
    update: UpdateState,
) -> Vec<ksni::MenuItem<Tray>> {
    let item = |key: Key, request: fn() -> Request| {
        ksni::menu::StandardItem {
            label: label(tr_in(language, key)),
            activate: Box::new(move |tray: &mut Tray| tray.send(request())),
            ..Default::default()
        }
        .into()
    };
    let languages = ksni::menu::RadioGroup {
        // Until one is picked, the language the system gave is the one shown, and ticked.
        selected: language as usize,
        // The language shown decides the mark: the resident moves it once the choice is saved.
        select: Box::new(|tray: &mut Tray, index| {
            if let Some(&choice) = Language::ALL.get(index) {
                tray.send(Request::Language(LanguageChoice::Fixed(choice)));
            }
        }),
        options: Language::ALL
            .into_iter()
            .map(|each| ksni::menu::RadioItem {
                label: label(each.native_name()),
                ..Default::default()
            })
            .collect(),
    };
    let mut menu = vec![
        item(Key::MenuCapture, || Request::Capture),
        item(Key::MenuInstantScreenshot, || Request::Instant),
    ];
    if let Some(checked) = magnifier {
        menu.push(
            ksni::menu::CheckmarkItem {
                label: label(tr_in(language, Key::MenuMagnifier)),
                checked,
                activate: Box::new(|tray: &mut Tray| tray.send(Request::Magnifier)),
                ..Default::default()
            }
            .into(),
        );
    }
    menu.extend([
        ksni::MenuItem::Separator,
        ksni::menu::SubMenu {
            label: label(&shotoshot_text::language_menu_label_in(language)),
            // The globe of the Windows menu, from the icon theme of the desktop.
            icon_name: "preferences-desktop-locale".into(),
            submenu: vec![languages.into()],
            ..Default::default()
        }
        .into(),
        update_item(language, update),
        ksni::MenuItem::Separator,
        item(Key::MenuQuit, || Request::Quit),
    ]);
    menu
}

/// The "Check for updates" line: greyed while the site is asked, "Download version X" once a
/// newer version was found.
fn update_item(language: Language, update: UpdateState) -> ksni::MenuItem<Tray> {
    let (text, request): (String, Option<fn() -> Request>) = match update {
        UpdateState::Available(version) => (
            Text::with_in(
                language,
                Key::MenuDownloadUpdate,
                &[("version", &version.to_string())],
            )
            .to_string(),
            Some(|| Request::OpenDownloadPage),
        ),
        UpdateState::Checking => (tr_in(language, Key::SettingsUpdatesChecking).into(), None),
        _ => (
            tr_in(language, Key::MenuCheckUpdates).into(),
            Some(|| Request::CheckUpdates),
        ),
    };
    ksni::menu::StandardItem {
        label: label(&text),
        enabled: request.is_some(),
        activate: Box::new(move |tray: &mut Tray| {
            if let Some(request) = request {
                tray.send(request());
            }
        }),
        ..Default::default()
    }
    .into()
}

/// Shows what the check found: a brief word when up to date, a longer one for a newer version
/// (the menu line now downloads it) or a failure. Without curl, the download page opens.
fn show_checked(handle: &ksni::blocking::Handle<Tray>, checked: Checked) {
    let running = running_version().to_string();
    let state = match checked {
        Checked::Found(Outcome::UpToDate) => {
            let text = Text::with(Key::SettingsUpdatesLatest, &[("version", &running)]);
            crate::notice::confirm(&format!("Shotoshot: {text}"));
            UpdateState::UpToDate
        }
        Checked::Found(Outcome::Available(version)) => {
            let text = Text::with(
                Key::SettingsUpdatesAvailable,
                &[("version", &version.to_string())],
            );
            crate::notice::report(&format!("Shotoshot: {text}"));
            UpdateState::Available(version)
        }
        Checked::Failed => {
            let text = shotoshot_text::tr(Key::SettingsUpdatesFailed);
            crate::notice::report(&format!("Shotoshot: {text}"));
            UpdateState::Failed
        }
        Checked::NoCurl => {
            open_download_page();
            UpdateState::Idle
        }
    };
    handle.update(|tray| tray.update = state);
}

/// The download page in the browser; a failure is reported.
fn open_download_page() {
    if let Err(error) = crate::update::open_download_page() {
        let message = Text::with(Key::ErrorUnexpected, &[("detail", &error.to_string())]);
        crate::notice::report(&format!("Shotoshot: {message}"));
    }
}

/// Switches labwc's magnifier, at the size and zoom of the pointer settings shared with
/// Windows. A failure is reported and leaves the line as the magnifier really is: off when
/// switching on failed (its blocks taken out again), off too when switching off failed, since
/// its blocks are gone either way.
fn switch_magnifier(handle: &ksni::blocking::Handle<Tray>, on: bool) {
    let result = if on {
        let settings = crate::settings::load().unwrap_or_default();
        crate::magnifier::switch_on(crate::magnifier::chosen(&settings)).inspect_err(|_| {
            let _ = crate::magnifier::forget();
        })
    } else {
        crate::magnifier::switch_off()
    };
    if let Err(error) = &result {
        let message = Text::with(Key::ErrorUnexpected, &[("detail", &error.to_string())]);
        crate::notice::report(&format!("Shotoshot: {message}"));
    }
    let now = on && result.is_ok();
    handle.update(|tray| tray.magnifier = Some(now));
}

/// Stores the language chosen in the menu, speaks it in this menu at once and in every capture
/// started after it (each one reads the settings). A refused save keeps the former language.
fn choose_language(handle: &ksni::blocking::Handle<Tray>, choice: LanguageChoice) {
    if let Err(error) = crate::settings::save_language(choice) {
        let message = Text::with(Key::ErrorSettingsUnsaved, &[("detail", &error)]);
        crate::notice::report(&format!("Shotoshot: {message}"));
        return;
    }
    shotoshot_text::set_language(choice.resolve(&crate::language::system_tag()));
    // The menu is built again, in the new language.
    handle.update(|_| {});
}

/// ARGB32 (network order) icons from the shipped PNGs.
fn icons() -> Vec<ksni::Icon> {
    [
        &include_bytes!("../../shotoshot-app/assets/shotoshot-32.png")[..],
        &include_bytes!("../../shotoshot-app/assets/shotoshot-48.png")[..],
    ]
    .into_iter()
    .filter_map(|png| crate::png_io::decode_rgba(png).ok())
    .map(|(width, height, rgba)| ksni::Icon {
        width: width as i32,
        height: height as i32,
        data: rgba
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[3], p[0], p[1], p[2]])
            .collect(),
    })
    .collect()
}

/// Returns false when another resident already runs in this session.
pub fn run() -> Result<bool, Box<dyn Error>> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").ok_or("XDG_RUNTIME_DIR is not set")?;
    let lock = std::fs::File::create(std::path::Path::new(&runtime).join("shotoshot.lock"))?;
    if lock.try_lock().is_err() {
        return Ok(false);
    }
    // The menu is built in the stored language; a settings file that cannot be read leaves the
    // session's language (the editor reports the damage when it
    // opens).
    let settings = crate::settings::load().unwrap_or_default();
    crate::language::apply(&settings);
    let (requests, received) = channel();
    // The update check answers through the same channel, from its own thread.
    let replies = requests.clone();
    // Blocks left by a resident that did not quit (a crash, a logout): the magnifier starts off.
    let labwc = crate::magnifier::available();
    if labwc {
        let _ = crate::magnifier::forget();
    }
    let tray = Tray {
        requests,
        icons: icons(),
        magnifier: labwc.then_some(false),
        update: UpdateState::Idle,
    };
    use ksni::blocking::TrayMethods;
    let handle = tray.spawn()?;
    let exe = std::env::current_exe()?;
    for request in received {
        let argument = match request {
            Request::Capture => "--capture",
            Request::Instant => "--instant",
            Request::Language(choice) => {
                choose_language(&handle, choice);
                continue;
            }
            Request::Magnifier => {
                let on = handle.update(|tray| tray.magnifier) == Some(Some(true));
                switch_magnifier(&handle, !on);
                continue;
            }
            Request::CheckUpdates => {
                // One check at a time: the greyed line refuses a second click anyway.
                let idle = handle.update(|tray| {
                    let idle = tray.update != UpdateState::Checking;
                    tray.update = UpdateState::Checking;
                    idle
                });
                if idle == Some(true) {
                    let replies = replies.clone();
                    std::thread::spawn(move || {
                        let checked = crate::update::check(running_version());
                        let _ = replies.send(Request::Checked(checked));
                    });
                }
                continue;
            }
            Request::Checked(checked) => {
                show_checked(&handle, checked);
                continue;
            }
            Request::OpenDownloadPage => {
                open_download_page();
                continue;
            }
            Request::Quit => break,
        };
        // Reap finished captures without blocking the menu.
        let mut child = Command::new(&exe).arg(argument).spawn()?;
        std::thread::spawn(move || child.wait());
    }
    if handle.update(|tray| tray.magnifier) == Some(Some(true)) {
        let _ = crate::magnifier::switch_off();
    }
    drop(lock);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(menu: &[ksni::MenuItem<Tray>]) -> Vec<String> {
        menu.iter()
            .filter_map(|item| match item {
                ksni::MenuItem::Standard(item) => Some(item.label.clone()),
                ksni::MenuItem::SubMenu(menu) => Some(menu.label.clone()),
                _ => None,
            })
            .collect()
    }
    fn radio(menu: &[ksni::MenuItem<Tray>]) -> (usize, Vec<String>) {
        let group = menu
            .iter()
            .find_map(|item| match item {
                ksni::MenuItem::SubMenu(menu) => menu.submenu.iter().find_map(|item| match item {
                    ksni::MenuItem::RadioGroup(group) => Some(group),
                    _ => None,
                }),
                _ => None,
            })
            .expect("a language submenu with its choices");
        let names = group.options.iter().map(|o| o.label.clone()).collect();
        (group.selected, names)
    }

    #[test]
    fn tray_labels_follow_the_language() {
        let idle = UpdateState::Idle;
        let english = labels(&menu_in(Language::En, None, idle));
        assert_eq!(
            english,
            [
                "Capture",
                "Instant screenshot",
                "Language",
                "Check for updates",
                "Quit Shotoshot"
            ]
        );
        let french = labels(&menu_in(Language::Fr, None, idle));
        assert_eq!(french[4], "Quitter Shotoshot");
        assert_eq!(french[3], "Rechercher des mises à jour");
        assert_eq!(french[2], "Langue · Language");
        for language in Language::ALL.into_iter().skip(1) {
            let translated = labels(&menu_in(language, None, idle));
            assert_ne!(translated[4], english[4], "{}", language.tag());
            assert!(translated[2].ends_with(" · Language"), "{}", language.tag());
        }
    }

    #[test]
    fn the_magnifier_line_shows_only_under_labwc() {
        let checked = |menu: &[ksni::MenuItem<Tray>]| {
            menu.iter().find_map(|item| match item {
                ksni::MenuItem::Checkmark(item) => Some((item.label.clone(), item.checked)),
                _ => None,
            })
        };
        let idle = UpdateState::Idle;
        assert_eq!(checked(&menu_in(Language::En, None, idle)), None);
        let on = menu_in(Language::En, Some(true), idle);
        assert_eq!(checked(&on), Some(("Magnifier".into(), true)));
        let off = menu_in(Language::Fr, Some(false), idle);
        assert_eq!(checked(&off).map(|(_, c)| c), Some(false));
        // Right under the captures, as the switch of the Windows menu sits among its effects.
        assert!(matches!(off[2], ksni::MenuItem::Checkmark(_)));
    }

    #[test]
    fn language_radio_lists_the_languages_and_marks_the_one_shown() {
        let natives: Vec<&str> = Language::ALL.iter().map(|l| l.native_name()).collect();
        for language in Language::ALL {
            let (selected, names) = radio(&menu_in(language, None, UpdateState::Idle));
            // No "system language" row: only the languages, under their own names.
            assert_eq!(names, natives, "{}", language.tag());
            assert_eq!(Language::ALL[selected], language);
        }
    }

    /// The line checks on click, greys while the site is asked, then downloads a newer version.
    #[test]
    fn the_update_line_checks_then_offers_the_download() {
        let line = |update| {
            menu_in(Language::En, None, update)
                .into_iter()
                .find_map(|item| match item {
                    ksni::MenuItem::Standard(item)
                        if item.label != "Capture"
                            && item.label != "Instant screenshot"
                            && item.label != "Quit Shotoshot" =>
                    {
                        Some((item.label, item.enabled))
                    }
                    _ => None,
                })
                .expect("an update line")
        };
        assert_eq!(line(UpdateState::Idle), ("Check for updates".into(), true));
        assert_eq!(line(UpdateState::Checking), ("Checking…".into(), false));
        assert_eq!(
            line(UpdateState::Failed),
            ("Check for updates".into(), true)
        );
        let newer = shotoshot_core::update::Version::parse("0.4.0").unwrap();
        assert_eq!(
            line(UpdateState::Available(newer)),
            ("Download version 0.4.0".into(), true)
        );
    }
}
