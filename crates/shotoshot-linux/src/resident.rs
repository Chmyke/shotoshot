//! ROLE: Linux resident: notification-area icon and its menu. Each capture runs in its own
//! short-lived process (`--capture`, `--instant`), so the resident stays small at rest.
//! INVARIANTS: One resident per user session (lock in XDG_RUNTIME_DIR).
use std::{
    error::Error,
    process::Command,
    sync::mpsc::{Sender, channel},
};

enum Request {
    Capture,
    Instant,
    Quit,
}

struct Tray {
    requests: Sender<Request>,
    icons: Vec<ksni::Icon>,
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
        let item = |label: &str, request: fn() -> Request| {
            ksni::menu::StandardItem {
                label: label.into(),
                activate: Box::new(move |tray: &mut Self| tray.send(request())),
                ..Default::default()
            }
            .into()
        };
        vec![
            item("Capture", || Request::Capture),
            item("Instant capture", || Request::Instant),
            ksni::MenuItem::Separator,
            item("Quit Shotoshot", || Request::Quit),
        ]
    }
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
    let (requests, received) = channel();
    let tray = Tray {
        requests,
        icons: icons(),
    };
    use ksni::blocking::TrayMethods;
    let _handle = tray.spawn()?;
    let exe = std::env::current_exe()?;
    for request in received {
        let argument = match request {
            Request::Capture => "--capture",
            Request::Instant => "--instant",
            Request::Quit => break,
        };
        // Reap finished captures without blocking the menu.
        let mut child = Command::new(&exe).arg(argument).spawn()?;
        std::thread::spawn(move || child.wait());
    }
    drop(lock);
    Ok(true)
}
