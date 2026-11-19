//! ROLE: The first-install window on Linux: proposes Print Screen as the capture key, records
//! another one if the user presses it, and binds it in the desktop's own shortcuts on Save.
//! The same window as on Windows (`shotoshot_ui::shortcut_window`), drawn by `shotoshot-raster`.
//! OWNER: The resident starts it in its own process (`--setup`) while no choice is recorded
//! (`desktop_shortcut::needs_setup`): winit allows one event loop per process.
//! INVARIANTS: Cancel, Escape or closing changes nothing, so the next start asks again. Only
//! keys every desktop names are recorded (`desktop_shortcut::keysym`); Super is not offered.
use crate::{desktop_shortcut, editor::present, keymap, notice};
use shotoshot_core::desktop_shortcut::{ALT, CTRL, Desktop, PRINT_SCREEN, SHIFT, keysym};
use shotoshot_raster::{Raster, present::Presenter};
use shotoshot_text::{Key, Language, Text, language, tr_in};
use shotoshot_ui::{
    icon::Icon,
    shortcut_window::{self as window, HINT_SIZE, Layout, View},
};
use std::{
    error::Error,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};
use tiny_skia::Pixmap;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalPosition},
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::ModifiersState,
    window::{Window, WindowId},
};

/// `shotoshot --setup` shows the window.
pub const ARG: &str = "--setup";
/// How long the probe shows the window before closing it, saving nothing.
const PROBE_TIME: Duration = Duration::from_millis(1500);

/// The name of a key the table does not name: letters and digits as printed.
fn layout_name(key: u32) -> Option<String> {
    matches!(key, 48..=57 | 65..=90).then(|| char::from(key as u8).to_string())
}

/// What the window shows and records.
struct State {
    language: Language,
    pending: Option<(u32, u32)>,
    recording: bool,
    focus: usize,
    held: u32,
    error: String,
}
impl State {
    /// Print Screen proposed, Save focused: Enter accepts it, any other key replaces it.
    fn first_run() -> Self {
        Self {
            language: language(),
            pending: Some((0, PRINT_SCREEN)),
            recording: false,
            focus: 2,
            held: 0,
            error: String::new(),
        }
    }
    fn hint(&self) -> String {
        if !self.error.is_empty() {
            return self.error.clone();
        }
        let key = if self.recording {
            Key::SetupLinuxAnyKeyHint
        } else if self.pending == Some((0, PRINT_SCREEN)) {
            Key::SetupLinuxPrintScreenHint
        } else {
            Key::SetupChangeHint
        };
        tr_in(self.language, key).into()
    }
    /// Records `key` with the modifiers held; the keys no desktop names are ignored.
    fn record(&mut self, key: u32) -> bool {
        if keysym(key).is_none() {
            return false;
        }
        self.pending = Some((self.held, key));
        self.recording = false;
        self.focus = 2;
        self.error.clear();
        true
    }
    fn label(&self, (mods, key): (u32, u32)) -> String {
        window::key_parts_in(mods, key, self.language, &layout_name).join("+")
    }
}

/// The window sized for every text it may show in `state.language`, its current error included.
fn layout(raster: &mut Raster, state: &State) -> Layout {
    let language = state.language;
    let title = tr_in(language, Key::SetupTitleCapture);
    let mut hints: Vec<String> = [
        Key::SetupLinuxPrintScreenHint,
        Key::SetupLinuxAnyKeyHint,
        Key::SetupChangeHint,
    ]
    .into_iter()
    .map(|key| tr_in(language, key).to_string())
    .collect();
    hints.push(state.error.clone());
    // Both measures use the one raster, never at the same time.
    let raster = std::cell::RefCell::new(raster);
    let measured: Result<Layout, ()> = Layout::measured(
        language,
        &[title],
        &hints,
        &layout_name,
        &mut |text, size| Ok(raster.borrow_mut().measure_text(text, size).x),
        &mut |text, size, width| {
            let mut raster = raster.borrow_mut();
            let lines = raster.wrapped_text(text, size, width);
            Ok(raster.measure_text(&lines, size).y)
        },
    );
    measured.unwrap_or_default()
}

/// The frame for a window `width`×`height` device pixels showing `state`.
fn frame(
    raster: &mut Raster,
    state: &State,
    layout: Layout,
    width: u32,
    height: u32,
) -> Result<Pixmap, Box<dyn Error>> {
    let mut pixmap = Pixmap::new(width.max(1), height.max(1)).ok_or("window too large")?;
    let scale = width as f64 / layout.size.x;
    // `Canvas::text` never breaks a line: the hint arrives broken where its zone ends.
    let hint = raster.wrapped_text(
        &state.hint(),
        HINT_SIZE * scale,
        layout.hint.size().x * scale,
    );
    let view = View {
        language: state.language,
        layout,
        icon: Icon::Capture,
        title: tr_in(state.language, Key::SetupTitleCapture),
        pieces: window::pieces(
            state.recording,
            state.held,
            state.pending,
            state.language,
            &layout_name,
        ),
        hint: &hint,
        error: !state.error.is_empty(),
        field_framed: state.recording || state.focus == 0,
        focus: state.focus,
        save_enabled: state.pending.is_some() && !state.recording,
    };
    window::paint(&mut raster.canvas(&mut pixmap), view, scale).map_err(|e| e.0)?;
    Ok(pixmap)
}

struct App {
    state: State,
    desktop: Desktop,
    raster: Raster,
    layout: Layout,
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    presenter: Presenter,
    cursor: PhysicalPosition<f64>,
    /// The probe: where the first frame goes, and when the window closes by itself.
    probe: Option<(PathBuf, Instant)>,
    error: Option<Box<dyn Error>>,
}

/// Shows the window until the key is saved or the window dismissed. `probe`: the folder that
/// receives the first frame; the window then closes after a moment, saving nothing.
pub fn run(probe: Option<PathBuf>) -> Result<(), Box<dyn Error>> {
    let desktop = desktop_shortcut::desktop().ok_or("no desktop Shotoshot can set")?;
    let state = State::first_run();
    let mut raster = Raster::new();
    let layout = layout(&mut raster, &state);
    let mut app = App {
        state,
        desktop,
        raster,
        layout,
        window: None,
        surface: None,
        presenter: Presenter::new(),
        cursor: PhysicalPosition::default(),
        probe: probe.map(|out| (out, Instant::now() + PROBE_TIME)),
        error: None,
    };
    EventLoop::new()?.run_app(&mut app)?;
    app.error.map_or(Ok(()), Err)
}

impl App {
    fn scale(&self) -> f64 {
        self.window
            .as_ref()
            .map_or(1., |w| w.inner_size().width as f64 / self.layout.size.x)
    }
    fn redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
    fn paint(&mut self) -> Result<(), Box<dyn Error>> {
        let (Some(window), Some(surface)) = (&self.window, &mut self.surface) else {
            return Ok(());
        };
        let size = window.inner_size();
        let pixmap = frame(
            &mut self.raster,
            &self.state,
            self.layout,
            size.width,
            size.height,
        )?;
        present(&mut self.presenter, &pixmap, window, surface)?;
        if let Some((out, _)) = &self.probe {
            std::fs::create_dir_all(out)?;
            pixmap.save_png(out.join("setup.png"))?;
        }
        Ok(())
    }
    /// 0 the field (listen), 1 Cancel, 2 Save.
    fn activate(&mut self, event_loop: &ActiveEventLoop, action: usize) {
        self.state.focus = action;
        match action {
            0 => {
                self.state.recording = true;
                self.state.held = 0;
                self.state.error.clear();
            }
            1 => event_loop.exit(),
            _ => self.save(event_loop),
        }
        self.redraw();
    }
    fn save(&mut self, event_loop: &ActiveEventLoop) {
        let Some((mods, key)) = self.state.pending.filter(|_| !self.state.recording) else {
            return;
        };
        match desktop_shortcut::apply(self.desktop, mods, key) {
            Ok(()) => {
                let shortcut = self.state.label((mods, key));
                let message = Text::with(Key::NoticeLinuxShortcutSet, &[("shortcut", &shortcut)]);
                notice::confirm(&message.to_string());
                event_loop.exit();
            }
            Err(error) => {
                let detail = error.to_string();
                self.state.error =
                    Text::with(Key::SetupLinuxRefused, &[("detail", &detail)]).to_string();
                self.relayout();
            }
        }
    }
    /// Sizes the window again for an error that needs more room.
    fn relayout(&mut self) {
        let layout = layout(&mut self.raster, &self.state);
        if layout != self.layout {
            self.layout = layout;
            if let Some(window) = &self.window {
                let _ = window.request_inner_size(LogicalSize::new(layout.size.x, layout.size.y));
            }
        }
    }
    fn key(&mut self, event_loop: &ActiveEventLoop, key: Option<u32>) {
        let Some(key) = key else { return };
        if self.state.recording {
            if key == 27 {
                self.state.recording = false;
            } else {
                self.state.record(key);
            }
        } else {
            match key {
                9 => {
                    let step = if self.state.held & SHIFT != 0 { 2 } else { 1 };
                    self.state.focus = (self.state.focus + step) % 3;
                }
                13 | 32 => return self.activate(event_loop, self.state.focus),
                27 => return event_loop.exit(),
                // Any other key is the new shortcut: no need to click the field first.
                _ => {
                    self.state.record(key);
                }
            }
        }
        self.redraw();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let title = tr_in(self.state.language, Key::SetupTitleCapture);
        let attributes = Window::default_attributes()
            .with_title(format!("{title} - Shotoshot"))
            .with_resizable(false)
            .with_inner_size(LogicalSize::new(self.layout.size.x, self.layout.size.y));
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
                if let Err(error) = self.paint() {
                    self.error = Some(error);
                    event_loop.exit();
                }
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => self.redraw(),
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::CursorMoved { position, .. } => self.cursor = position,
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                let scale = self.scale();
                if let Some(action) = self
                    .layout
                    .hit(self.cursor.x / scale, self.cursor.y / scale)
                {
                    self.activate(event_loop, action);
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let state: ModifiersState = modifiers.state();
                self.state.held = [
                    (state.control_key(), CTRL),
                    (state.shift_key(), SHIFT),
                    (state.alt_key(), ALT),
                ]
                .into_iter()
                .filter(|(down, _)| *down)
                .fold(0, |mods, (_, bit)| mods | bit);
                if self.state.recording {
                    self.redraw();
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let key = keymap::shortcut_key(&event.logical_key, event.physical_key);
                self.key(event_loop, key);
            }
            _ => {}
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some((_, deadline)) = self.probe {
            if Instant::now() >= deadline {
                event_loop.exit();
            } else {
                event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_screen_is_proposed_and_saved_with_enter() {
        let state = State::first_run();
        assert_eq!(state.pending, Some((0, PRINT_SCREEN)));
        assert_eq!(state.focus, 2);
        assert_eq!(
            state.hint(),
            tr_in(state.language, Key::SetupLinuxPrintScreenHint)
        );
    }

    #[test]
    fn another_key_replaces_it_and_unnamed_keys_are_ignored() {
        let mut state = State::first_run();
        state.held = CTRL | SHIFT;
        assert!(state.record(0x41));
        assert_eq!(state.pending, Some((CTRL | SHIFT, 0x41)));
        assert_eq!(state.label((CTRL | SHIFT, 0x41)).matches('+').count(), 2);
        // The Windows key and the numeric keypad have no name every desktop reads.
        assert!(!state.record(0x5B));
        assert!(!state.record(96));
        assert_eq!(state.pending, Some((CTRL | SHIFT, 0x41)));
    }

    #[test]
    fn every_language_fits_and_paints_inside_the_window() {
        let mut raster = Raster::new();
        for language in Language::ALL {
            let mut state = State::first_run();
            state.language = language;
            let layout = layout(&mut raster, &state);
            assert!(
                layout.size.x <= 640. && layout.size.y <= 400.,
                "{}: {:?}",
                language.tag(),
                layout.size
            );
            let hint = state.hint();
            let lines = raster.wrapped_text(&hint, HINT_SIZE, layout.hint.size().x);
            assert!(
                raster.measure_text(&lines, HINT_SIZE).y <= layout.hint.size().y,
                "{}: the hint leaves its zone",
                language.tag()
            );
            let (w, h) = (layout.size.x as u32, layout.size.y as u32);
            frame(&mut raster, &state, layout, w * 2, h * 2).unwrap();
        }
    }

    #[test]
    fn a_long_refusal_grows_the_window() {
        let mut raster = Raster::new();
        let mut state = State::first_run();
        let before = layout(&mut raster, &state);
        state.error = "gsettings set: ".repeat(20);
        assert!(layout(&mut raster, &state).size.y > before.size.y);
    }
}
