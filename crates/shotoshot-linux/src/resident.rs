//! ROLE: Linux resident: notification-area icon and its menu. Each capture runs in its own
//! short-lived process (`--capture`, `--instant`), so the resident stays small at rest.
//! INVARIANTS: One resident per user session (lock in XDG_RUNTIME_DIR).
use shotoshot_text::{Key, Language, LanguageChoice, Text, language, tr_in};
use std::{
    error::Error,
    process::Command,
    sync::mpsc::{Sender, channel},
};

enum Request {
    Capture,
    Instant,
    Language(LanguageChoice),
    Quit,
}

struct Tray {
    requests: Sender<Request>,
    icons: Vec<ksni::Icon>,
    /// The stored choice, marked in the language submenu.
    language: LanguageChoice,
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
        menu_in(language(), self.language)
    }
}

/// A text as a menu label: an underscore would otherwise mark an access key.
fn label(text: &str) -> String {
    text.replace('_', "__")
}

/// The name of `choice` in the language submenu: the system language in `language`, or a
/// language under its own name.
fn choice_label_in(choice: LanguageChoice, language: Language) -> String {
    match choice {
        LanguageChoice::Auto => label(tr_in(language, Key::MenuLanguageSystem)),
        LanguageChoice::Fixed(fixed) => label(fixed.native_name()),
    }
}

/// The menu in `language`, with `chosen` marked in the language submenu.
fn menu_in(language: Language, chosen: LanguageChoice) -> Vec<ksni::MenuItem<Tray>> {
    let item = |key: Key, request: fn() -> Request| {
        ksni::menu::StandardItem {
            label: label(tr_in(language, key)),
            activate: Box::new(move |tray: &mut Tray| tray.send(request())),
            ..Default::default()
        }
        .into()
    };
    let languages = ksni::menu::RadioGroup {
        selected: chosen.position(),
        // The stored setting decides the mark: the resident moves it once the choice is saved.
        select: Box::new(|tray: &mut Tray, index| {
            if let Some(&choice) = LanguageChoice::ALL.get(index) {
                tray.send(Request::Language(choice));
            }
        }),
        options: LanguageChoice::ALL
            .into_iter()
            .map(|choice| ksni::menu::RadioItem {
                label: choice_label_in(choice, language),
                ..Default::default()
            })
            .collect(),
    };
    vec![
        item(Key::MenuCapture, || Request::Capture),
        item(Key::MenuInstantScreenshot, || Request::Instant),
        ksni::MenuItem::Separator,
        ksni::menu::SubMenu {
            label: label(&shotoshot_text::language_menu_label_in(language)),
            // The globe of the Windows menu, from the icon theme of the desktop.
            icon_name: "preferences-desktop-locale".into(),
            submenu: vec![languages.into()],
            ..Default::default()
        }
        .into(),
        ksni::MenuItem::Separator,
        item(Key::MenuQuit, || Request::Quit),
    ]
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
    handle.update(|tray| tray.language = choice);
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
    // session's language and marks the automatic choice (the editor reports the damage when it
    // opens).
    let settings = crate::settings::load().unwrap_or_default();
    crate::language::apply(&settings);
    let (requests, received) = channel();
    let tray = Tray {
        requests,
        icons: icons(),
        language: settings.language,
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
            Request::Quit => break,
        };
        // Reap finished captures without blocking the menu.
        let mut child = Command::new(&exe).arg(argument).spawn()?;
        std::thread::spawn(move || child.wait());
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
        let english = labels(&menu_in(Language::En, LanguageChoice::Auto));
        assert_eq!(
            english,
            [
                "Capture",
                "Instant screenshot",
                "Language",
                "Quit Shotoshot"
            ]
        );
        let french = labels(&menu_in(Language::Fr, LanguageChoice::Auto));
        assert_eq!(french[3], "Quitter Shotoshot");
        assert_eq!(french[2], "Langue · Language");
        for language in Language::ALL.into_iter().skip(1) {
            let translated = labels(&menu_in(language, LanguageChoice::Auto));
            assert_ne!(translated[3], english[3], "{}", language.tag());
            assert!(translated[2].ends_with(" · Language"), "{}", language.tag());
        }
    }

    #[test]
    fn language_radio_marks_the_setting() {
        let (selected, names) = radio(&menu_in(Language::En, LanguageChoice::Auto));
        assert_eq!(selected, 0);
        assert_eq!(names.len(), 1 + Language::ALL.len());
        assert_eq!(names[0], "System language");
        let natives: Vec<&str> = Language::ALL.iter().map(|l| l.native_name()).collect();
        assert_eq!(names[1..], natives[..]);
        let (_, french) = radio(&menu_in(Language::Fr, LanguageChoice::Auto));
        assert_eq!(french[0], "Langue du système");
        assert_eq!(french[1..], natives[..], "native names never change");
        for choice in LanguageChoice::ALL {
            let (selected, _) = radio(&menu_in(Language::De, choice));
            assert_eq!(LanguageChoice::ALL[selected], choice);
        }
    }
}
