//! ROLE: Message window for what happens outside the editor: failures (capture refused,
//! instant capture) and the confirmation of an action that closed it.
//! Linux desktops may lack a notification service (Raspberry Pi OS has none), so Shotoshot
//! shows its own, as Windows does.
//! OWNER: `lib::run` on error, the editor after a save. Closes after its delay, a click or
//! a key.
use crate::editor::present;
use shotoshot_core::{Point, Rect};
use shotoshot_raster::Raster;
use shotoshot_ui::{draw_notice, notice_size};
use std::{
    error::Error,
    rc::Rc,
    time::{Duration, Instant},
};
use tiny_skia::{Color, Pixmap};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

/// How long a failure stays up unless dismissed.
pub const DURATION: Duration = Duration::from_secs(6);
/// How long a confirmation stays up: read at a glance, as on Windows.
pub const BRIEF: Duration = Duration::from_secs(2);
/// `shotoshot --notice <message>` shows the window and exits.
pub const ARG: &str = "--notice";
/// `shotoshot --confirmation <message>` does the same, briefly.
pub const CONFIRMATION: &str = "--confirmation";

/// Shows `message` from its own process: winit allows one event loop per process, and
/// the one that speaks (editor, capture) may already have used it. `kind` is `ARG` or
/// `CONFIRMATION`.
pub fn spawn(kind: &str, message: &str) -> std::io::Result<std::process::Child> {
    std::process::Command::new(std::env::current_exe()?)
        .arg(kind)
        .arg(message)
        .spawn()
}
/// A failure in its window, falling back to the terminal when no process can start.
pub fn report(message: &str) {
    if let Err(error) = spawn(ARG, message) {
        eprintln!("{message} (message window unavailable: {error})");
    }
}
/// A confirmation in its window. The action it confirms is done either way.
pub fn confirm(message: &str) {
    if let Err(error) = spawn(CONFIRMATION, message) {
        eprintln!("{message} (message window unavailable: {error})");
    }
}

struct Notice {
    frame: Pixmap,
    deadline: Instant,
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    error: Option<Box<dyn Error>>,
}

pub fn show(message: &str, duration: Duration) -> Result<(), Box<dyn Error>> {
    let mut raster = Raster::new();
    let mut scratch = Pixmap::new(1, 1).ok_or("no pixmap")?;
    // Sized before any window exists: the monitor scale is not known yet.
    let scale = 1.;
    let size = notice_size(&mut raster.canvas(&mut scratch), message, scale).map_err(|e| e.0)?;
    let (width, height) = (size.x.ceil() as u32, size.y.ceil() as u32);
    let mut frame = Pixmap::new(width, height).ok_or("message too large")?;
    // The window is opaque: its corners take the box colour.
    frame.fill(Color::from_rgba8(25, 25, 28, 255));
    let bounds = Rect {
        min: Point::default(),
        max: Point {
            x: width as f64,
            y: height as f64,
        },
    };
    draw_notice(&mut raster.canvas(&mut frame), bounds, message, scale).map_err(|e| e.0)?;
    let mut notice = Notice {
        frame,
        deadline: Instant::now() + duration,
        window: None,
        surface: None,
        error: None,
    };
    EventLoop::new()?.run_app(&mut notice)?;
    notice.error.map_or(Ok(()), Err)
}

impl ApplicationHandler for Notice {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("Shotoshot")
            .with_decorations(false)
            .with_resizable(false)
            .with_inner_size(PhysicalSize::new(self.frame.width(), self.frame.height()));
        let created = event_loop.create_window(attributes).map(Rc::new);
        let result = created.map_err(Box::<dyn Error>::from).and_then(|window| {
            let context = softbuffer::Context::new(window.clone())?;
            self.surface = Some(softbuffer::Surface::new(&context, window.clone())?);
            window.request_redraw();
            self.window = Some(window);
            Ok(())
        });
        if let Err(error) = result {
            self.error = Some(error);
            event_loop.exit();
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::RedrawRequested => {
                if let (Some(window), Some(surface)) = (&self.window, &mut self.surface)
                    && let Err(error) = present(&self.frame, window, surface)
                {
                    self.error = Some(error);
                    event_loop.exit();
                }
            }
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                ..
            } => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                event_loop.exit()
            }
            _ => {}
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if Instant::now() >= self.deadline {
            event_loop.exit();
        } else {
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.deadline));
        }
    }
}
