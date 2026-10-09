//! ROLE: The Linux capture editor: a full-screen winit window over the frozen capture,
//! painted by `shotoshot-raster` and driven by the portable `Capture`.
//! OWNER: One editor per capture; the process ends with it.
//! INVARIANTS: Window pixels map to capture pixels by a per-axis scale (1:1 on one monitor).
use crate::{clipboard, keymap, monitors, notice, png_io, settings};
use shotoshot_core::{
    Modifiers, Point,
    settings::{SaveFormat, Settings},
};
use shotoshot_raster::{Raster, pixmap_from_bgra};
use shotoshot_ui::{
    Canvas, Capture, CursorKind, DimensionFields, DimensionKey, FieldKey, Host, Image, Monitor,
    TextField, capture_name, draw_notice, keys::KeyOutcome, notice_size, toolbar::Action,
    top_center, wheel,
};
use std::{error::Error, num::NonZeroU32, path::PathBuf, rc::Rc, time::Instant};
use tiny_skia::Pixmap;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::ModifiersState,
    monitor::MonitorHandle,
    window::{CursorIcon, Fullscreen, Window, WindowId},
};

/// Native side of the Linux editor: renderer resources and the portable inline fields.
struct Native {
    raster: Raster,
    field: Option<TextField>,
    dimensions: Option<DimensionFields>,
    modifiers: Modifiers,
}
impl Native {
    fn measure_dimension(raster: &mut Raster, size: f64) -> impl FnMut(&str) -> Point + '_ {
        move |s| raster.measure_text(s, size)
    }
}
impl Host for Native {
    type Error = Box<dyn Error>;
    // winit keeps delivering pointer events to the window while a button is held.
    fn capture_pointer(&mut self) {}
    fn release_pointer(&mut self) {}
    fn focus_text(&mut self) {}
    fn text_active(&self) -> bool {
        self.field.is_some()
    }
    fn set_text_color(&mut self, rgb: [u8; 3]) {
        if let Some(field) = &mut self.field {
            field.color = rgb;
        }
    }
    fn cancel_text_drag(&mut self) -> bool {
        self.field.as_mut().is_some_and(|f| f.pointer_up())
    }
    fn begin_text(&mut self, capture: &mut Capture, p: Point) -> Result<(), Self::Error> {
        let mut field = TextField::open(capture, p);
        if field.object.is_some() {
            let (raster, size) = (&mut self.raster, field.size as f64);
            field.pointer_down(p, false, &mut |s| raster.measure_annotation_text(s, size));
            field.pointer_up();
        }
        self.field = Some(field);
        Ok(())
    }
    fn finish_text(&mut self, capture: &mut Capture, commit: bool) {
        if let Some(field) = self.field.take()
            && commit
        {
            let extent = self
                .raster
                .measure_annotation_text(field.text(), field.size as f64);
            field.commit(capture, extent);
        }
    }
    fn text_pointer_down(&mut self, p: Point) -> Result<bool, Self::Error> {
        let extend = self.modifiers.shift;
        let Some(field) = &mut self.field else {
            return Ok(false);
        };
        let (raster, size) = (&mut self.raster, field.size as f64);
        Ok(field.pointer_down(p, extend, &mut |s| raster.measure_annotation_text(s, size)))
    }
    fn text_pointer_move(&mut self, p: Point) -> bool {
        let Some(field) = &mut self.field else {
            return false;
        };
        let (raster, size) = (&mut self.raster, field.size as f64);
        field.pointer_move(p, &mut |s| raster.measure_annotation_text(s, size))
    }
    fn text_pointer_up(&mut self) -> bool {
        self.field.as_mut().is_some_and(|f| f.pointer_up())
    }
    fn text_size(&self) -> Option<f32> {
        self.field.as_ref().map(|field| field.size)
    }
    fn set_text_size(&mut self, size: f32) -> Result<(), Self::Error> {
        if let Some(field) = &mut self.field {
            field.size = size;
        }
        Ok(())
    }
    fn measure_text(&mut self, text: &str, size: f64) -> Result<Point, Self::Error> {
        Ok(self.raster.measure_annotation_text(text, size))
    }
    fn dimensions_active(&self) -> bool {
        self.dimensions.is_some()
    }
    fn begin_dimensions(&mut self, capture: &mut Capture) -> Result<(), Self::Error> {
        self.finish_text(capture, true);
        capture.editor.cancel();
        self.dimensions = DimensionFields::open(capture);
        Ok(())
    }
    fn finish_dimensions(&mut self, capture: &mut Capture, commit: bool) -> bool {
        let Some(fields) = &mut self.dimensions else {
            return true;
        };
        if commit && !fields.apply(capture) {
            return false;
        }
        self.dimensions = None;
        true
    }
    fn dimension_pointer_down(&mut self, _: &mut Capture, p: Point) -> Result<bool, Self::Error> {
        let extend = self.modifiers.shift;
        let Some(fields) = &mut self.dimensions else {
            return Ok(false);
        };
        let mut measure = Self::measure_dimension(&mut self.raster, fields.font_size());
        Ok(fields.pointer_down(p, extend, &mut measure))
    }
    fn dimension_pointer_move(&mut self, p: Point) {
        if let Some(fields) = &mut self.dimensions {
            let mut measure = Self::measure_dimension(&mut self.raster, fields.font_size());
            fields.pointer_move(p, &mut measure);
        }
    }
    fn dimension_pointer_up(&mut self) -> bool {
        self.dimensions.as_mut().is_some_and(|f| {
            f.pointer_up();
            true
        })
    }
    // `Editor::key` sends keys to open fields first; this only keeps the rest from them.
    fn dimension_key(&mut self, _: &mut Capture, _: u32) -> bool {
        self.dimensions.is_some()
    }
}

/// Scripted run for `--linux-diagnostic`: gestures, then evidence files, then exit.
struct Diagnostic {
    out: PathBuf,
    step: u8,
}

struct Editor {
    capture: Capture,
    native: Native,
    backdrop: Pixmap,
    frame: Pixmap,
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    diagnostic: Option<Diagnostic>,
    /// Preferences at start; the stroke width and custom colour are stored back on exit.
    settings: Settings,
    /// False in diagnostics and when the settings file could not be read (kept as is).
    persist: bool,
    /// Why the last action failed; shown at the top until the next press or key.
    notice: Option<String>,
    error: Option<Box<dyn Error>>,
}

pub fn run(image: Image, diagnostic: Option<PathBuf>) -> Result<(), Box<dyn Error>> {
    let backdrop = pixmap_from_bgra(&image).map_err(|e| e.0)?;
    // Diagnostics run with defaults and never touch the user's preferences.
    let (settings, persist, notice) = match diagnostic {
        Some(_) => (Settings::default(), false, None),
        None => match settings::load() {
            Ok(settings) => (settings, true, None),
            Err(error) => (
                Settings::default(),
                false,
                Some(format!("Settings could not be read: {error}")),
            ),
        },
    };
    let mut editor = Editor {
        frame: backdrop.clone(),
        backdrop,
        capture: prepared(image, &settings),
        native: Native {
            raster: Raster::new(),
            field: None,
            dimensions: None,
            modifiers: Modifiers::default(),
        },
        window: None,
        surface: None,
        diagnostic: diagnostic.map(|out| Diagnostic { out, step: 0 }),
        settings,
        persist,
        notice,
        error: None,
    };
    EventLoop::new()?.run_app(&mut editor)?;
    editor.persist_preferences();
    editor.error.map_or(Ok(()), Err)
}

/// A capture starting with the remembered stroke width and custom colour.
fn prepared(image: Image, settings: &Settings) -> Capture {
    let mut capture = Capture::new(image);
    capture.custom_color = settings.last_custom_color;
    capture.editor.style.width = settings.capture_stroke_width as f64;
    capture
}

/// Shows the opaque `frame` in `window`, scaled to its size (1:1 on one monitor).
pub(crate) fn present(
    frame: &Pixmap,
    window: &Window,
    surface: &mut softbuffer::Surface<Rc<Window>, Rc<Window>>,
) -> Result<(), Box<dyn Error>> {
    let size = window.inner_size();
    let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
        return Ok(());
    };
    surface.resize(w, h)?;
    let mut buffer = surface.buffer_mut()?;
    let (fw, fh) = (frame.width() as usize, frame.height() as usize);
    let (bw, bh) = (size.width as usize, size.height as usize);
    let data = frame.data();
    for y in 0..bh {
        let sy = y * fh / bh;
        for x in 0..bw {
            let sx = x * fw / bw;
            let i = (sy * fw + sx) * 4;
            // Opaque frame: premultiplied RGBA equals straight RGB.
            buffer[y * bw + x] =
                (data[i] as u32) << 16 | (data[i + 1] as u32) << 8 | data[i + 2] as u32;
        }
    }
    buffer.present()?;
    Ok(())
}

fn cursor_icon(kind: CursorKind) -> CursorIcon {
    match kind {
        CursorKind::Arrow => CursorIcon::Default,
        CursorKind::Cross => CursorIcon::Crosshair,
        CursorKind::IBeam => CursorIcon::Text,
        CursorKind::Hand => CursorIcon::Pointer,
        CursorKind::Move => CursorIcon::Move,
        CursorKind::ResizeNwse => CursorIcon::NwseResize,
        CursorKind::ResizeNesw => CursorIcon::NeswResize,
        CursorKind::ResizeNs => CursorIcon::NsResize,
        CursorKind::ResizeWe => CursorIcon::EwResize,
    }
}

impl Editor {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: Box<dyn Error>) {
        self.error.get_or_insert(error);
        event_loop.exit();
    }
    /// A failed action keeps the session open and says why, as on Windows; diagnostics
    /// stop on it instead.
    fn report(&mut self, event_loop: &ActiveEventLoop, error: Box<dyn Error>) {
        if self.diagnostic.is_some() {
            return self.fail(event_loop, error);
        }
        self.notice = Some(error.to_string());
        self.redraw();
    }
    /// Stores a changed stroke width or custom colour, keeping every other preference.
    fn persist_preferences(&mut self) {
        if !self.persist {
            return;
        }
        let width = self.capture.stroke_width_setting();
        let width = (width != self.settings.capture_stroke_width).then_some(width);
        let color = self
            .capture
            .custom_color_changed
            .then_some(self.capture.custom_color);
        if let Err(error) = settings::save_editor(width, color) {
            notice::report(&format!("Shotoshot: preferences not saved: {error}"));
        }
    }
    fn redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
    /// The window covers one monitor: its display scale sizes the whole interface.
    fn set_scale(&mut self, scale: f64) {
        let image = &self.capture.image;
        self.capture.monitors = vec![Monitor {
            bounds: shotoshot_core::Rect {
                min: Point::default(),
                max: Point {
                    x: image.width as f64,
                    y: image.height as f64,
                },
            },
            scale,
        }];
    }
    /// Window position to capture position.
    fn to_capture(&self, x: f64, y: f64) -> Point {
        let size = self.window.as_ref().map(|w| w.inner_size());
        let (w, h) = size.map_or((1., 1.), |s| {
            (s.width.max(1) as f64, s.height.max(1) as f64)
        });
        Point {
            x: x * self.capture.image.width as f64 / w,
            y: y * self.capture.image.height as f64 / h,
        }
    }
    fn paint(&mut self) -> Result<(), Box<dyn Error>> {
        self.frame.data_mut().copy_from_slice(self.backdrop.data());
        let capture = &self.capture;
        let native = &mut self.native;
        let mut canvas = native.raster.canvas(&mut self.frame);
        capture.draw_underlay(&mut canvas).map_err(|e| e.0)?;
        if capture.region.is_some() {
            let editing = native.field.as_ref().and_then(|f| f.object);
            canvas
                .annotations(capture.editor.objects(), editing)
                .map_err(|e| e.0)?;
            if let Some(field) = &native.field {
                field.draw(&mut canvas, true).map_err(|e| e.0)?;
            }
            capture
                .draw_frame(&mut canvas, native.field.is_none())
                .map_err(|e| e.0)?;
            if !capture.selecting {
                let current = capture
                    .picker
                    .as_ref()
                    .map(|p| p.color())
                    .unwrap_or_else(|| {
                        native
                            .field
                            .as_ref()
                            .map_or(capture.editor.style.color, |f| f.color)
                    });
                capture
                    .draw_toolbar(&mut canvas, current)
                    .map_err(|e| e.0)?;
                let over_toolbar = capture.toolbar_contains(capture.pointer);
                let feedback = capture.picker.is_none()
                    && (native.field.is_none() || capture.eyedropper)
                    && (!over_toolbar || capture.eyedropper);
                if feedback && capture.eyedropper {
                    capture.draw_magnifier(&mut canvas).map_err(|e| e.0)?;
                } else if feedback && capture.width_ring_visible() {
                    capture.draw_width_ring(&mut canvas).map_err(|e| e.0)?;
                }
                if let Some(picker) = &capture.picker {
                    picker.draw(&mut canvas).map_err(|e| e.0)?;
                }
            }
            let fields = native.dimensions.as_ref();
            capture
                .draw_dimensions(&mut canvas, fields.is_some(), fields.and_then(|f| f.error))
                .map_err(|e| e.0)?;
            if let Some(fields) = fields {
                fields.draw(&mut canvas).map_err(|e| e.0)?;
            }
            capture
                .draw_hover_hint(&mut canvas, Instant::now(), fields.is_some())
                .map_err(|e| e.0)?;
        }
        let screen = shotoshot_core::Rect {
            min: Point::default(),
            max: Point {
                x: capture.image.width as f64,
                y: capture.image.height as f64,
            },
        };
        capture
            .draw_selection_hint(&mut canvas, screen)
            .map_err(|e| e.0)?;
        if let Some(message) = &self.notice {
            let scale = capture.ui_scale();
            let size = notice_size(&mut canvas, message, scale).map_err(|e| e.0)?;
            let bounds = top_center(screen, size, scale);
            draw_notice(&mut canvas, bounds, message, scale).map_err(|e| e.0)?;
        }
        match (&self.window, &mut self.surface) {
            (Some(window), Some(surface)) => present(&self.frame, window, surface),
            _ => Ok(()),
        }
    }
    fn pointer_down(&mut self, p: Point, event_loop: &ActiveEventLoop) {
        self.notice = None;
        let modifiers = self.native.modifiers;
        match self.capture.pointer_down(&mut self.native, p, modifiers) {
            Ok(Some(action)) => self.run(action, event_loop),
            Ok(None) => {}
            Err(error) => self.fail(event_loop, error),
        }
    }
    fn pointer_move(&mut self, p: Point) {
        let modifiers = self.native.modifiers;
        self.capture.pointer_move(&mut self.native, p, modifiers);
        let target = self.capture.hover_target(p);
        self.capture.hover_hint.update(target, Instant::now());
        if let Some(window) = &self.window {
            let over_badge = !self.capture.selecting
                && self
                    .capture
                    .dimensions_bounds()
                    .is_some_and(|r| r.contains(p));
            window.set_cursor(cursor_icon(if over_badge {
                CursorKind::IBeam
            } else if self.capture.picker.is_some() {
                CursorKind::Arrow
            } else {
                self.capture.cursor(p)
            }));
        }
    }
    fn pointer_up(&mut self, p: Point) {
        let modifiers = self.native.modifiers;
        self.capture.pointer_up(&mut self.native, p, modifiers);
    }
    /// Applies a toolbar or shortcut action; the native ones are handled here.
    fn run(&mut self, action: Action, event_loop: &ActiveEventLoop) {
        let commit = !matches!(action, Action::Close);
        if !self.native.finish_dimensions(&mut self.capture, commit) {
            return;
        }
        match action {
            Action::Save => match self.save(None) {
                Ok(_) => event_loop.exit(),
                Err(error) => self.report(event_loop, error),
            },
            Action::Copy => {
                match self
                    .export()
                    .and_then(|image| clipboard::copy_image(&image))
                {
                    Ok(_) => event_loop.exit(),
                    Err(error) => self.report(event_loop, error),
                }
            }
            Action::CopyColor(rgb) => {
                let hex = format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
                if let Err(error) = clipboard::copy_text(&hex) {
                    self.report(event_loop, error);
                }
            }
            Action::Close => event_loop.exit(),
            // Not available on Linux yet: Google Lens and live capture.
            Action::GoogleSearch | Action::Live => {}
            other => self.capture.action(&mut self.native, other),
        }
    }
    fn export(&mut self) -> Result<Image, Box<dyn Error>> {
        let region = self.capture.region.ok_or("select a capture region first")?;
        let editor = &self.capture.editor;
        Ok(self
            .native
            .raster
            .export(
                &self.capture.image,
                editor.background(),
                editor.objects(),
                region,
            )
            .map_err(|e| e.0)?)
    }
    /// Saves the cropped capture as the next "Shot NNN.png" in `folder` (Pictures by default).
    fn save(&mut self, folder: Option<&std::path::Path>) -> Result<PathBuf, Box<dyn Error>> {
        let image = self.export()?;
        let folder = folder.map_or_else(pictures_dir, |f| f.to_path_buf());
        std::fs::create_dir_all(&folder)?;
        let path = capture_name::next_path(&folder, SaveFormat::Png)?;
        png_io::save(&image, &path)?;
        Ok(path)
    }
    fn key(&mut self, vk: Option<u32>, text: Option<&str>, event_loop: &ActiveEventLoop) {
        self.notice = None;
        let m = self.native.modifiers;
        if let Some(fields) = &mut self.native.dimensions {
            let raster = &mut self.native.raster;
            let size = fields.font_size();
            let mut measure = |s: &str| raster.measure_text(s, size);
            let key = vk.map(|vk| fields.key(vk, m, &mut measure));
            let mut insert = |s: &str| fields.insert(s, &mut measure);
            let result = match &key {
                Some(DimensionKey::Field(handled)) => {
                    follow_up(Some(handled.clone()), text, m.ctrl, &mut insert)
                }
                None => follow_up(None, text, m.ctrl, &mut insert),
                _ => Ok(()),
            };
            match key {
                Some(DimensionKey::Commit) => {
                    self.native.finish_dimensions(&mut self.capture, true);
                }
                Some(DimensionKey::Cancel) => {
                    self.native.finish_dimensions(&mut self.capture, false);
                }
                _ => {}
            }
            if let Err(error) = result {
                self.report(event_loop, error);
            }
            return;
        }
        if let Some(field) = &mut self.native.field {
            match vk {
                Some(27) => return self.native.finish_text(&mut self.capture, false),
                Some(13) if m.ctrl => return self.native.finish_text(&mut self.capture, true),
                _ => {}
            }
            let (raster, size) = (&mut self.native.raster, field.size as f64);
            let handled =
                vk.map(|vk| field.key(vk, m, &mut |s| raster.measure_annotation_text(s, size)));
            if let Err(error) = follow_up(handled, text, m.ctrl, &mut |s| field.insert(s)) {
                self.report(event_loop, error);
            }
            return;
        }
        let Some(vk) = vk else {
            return;
        };
        match self.capture.key(&mut self.native, vk, m) {
            KeyOutcome::Ignored | KeyOutcome::Handled => {}
            KeyOutcome::Action(a) => self.run(a, event_loop),
            KeyOutcome::CopyAnnotations => {
                let objects = self.capture.editor.copy_selected();
                if objects.is_empty() {
                    self.run(Action::Copy, event_loop);
                } else {
                    self.capture.copied_objects = objects;
                    self.capture.paste_count = 0;
                }
            }
            KeyOutcome::PasteAnnotations => {
                self.capture.paste_count = self.capture.paste_count.saturating_add(1);
                let offset = 16. * self.capture.paste_count as f64;
                let objects = self.capture.copied_objects.clone();
                self.capture.editor.paste_objects(
                    &objects,
                    Point {
                        x: offset,
                        y: offset,
                    },
                );
            }
            KeyOutcome::Close => event_loop.exit(),
        }
    }
    /// One scripted interaction per frame, so each state is really presented.
    fn diagnostic_step(&mut self, event_loop: &ActiveEventLoop) -> Result<(), Box<dyn Error>> {
        let Some(step) = self.diagnostic.as_ref().map(|d| d.step) else {
            return Ok(());
        };
        let (w, h) = (
            self.capture.image.width as f64,
            self.capture.image.height as f64,
        );
        let at = |fx: f64, fy: f64| Point {
            x: w * fx,
            y: h * fy,
        };
        match step {
            0 => {
                self.pointer_down(at(0.2, 0.2), event_loop);
                self.pointer_move(at(0.7, 0.65));
                self.pointer_up(at(0.7, 0.65));
            }
            1 => {
                // Size badge: width by expression, height after Tab.
                let bounds = self.capture.dimensions_bounds().ok_or("no size badge")?;
                let size = self.capture.region.ok_or("no region")?.size();
                let press = bounds.min + Point { x: 10., y: 10. };
                self.pointer_down(press, event_loop);
                self.pointer_up(press);
                let (w, h) = (size.x.round(), size.y.round());
                self.key(None, Some(&format!("{w}-40")), event_loop);
                self.key(Some(9), None, event_loop);
                self.key(None, Some(&format!("({h}+10)-10")), event_loop);
            }
            2 => {
                let out = self.diagnostic.as_ref().map(|d| d.out.clone());
                let frame = shotoshot_raster::image_from_pixmap(&self.frame, 0, 0);
                png_io::save(&frame, &out.unwrap_or_default().join("dimensions.png"))?;
                let typed = self
                    .native
                    .dimensions
                    .as_ref()
                    .ok_or("size fields closed")?;
                let [w, h] = typed.texts().map(str::to_owned);
                let before = self.capture.region.ok_or("no region")?.size();
                self.key(Some(13), None, event_loop);
                if self.native.dimensions.is_some() {
                    return Err(format!("size fields refused {w} x {h}").into());
                }
                let after = self.capture.region.ok_or("no region")?.size();
                if after
                    != (Point {
                        x: before.x.round() - 40.,
                        y: before.y.round(),
                    })
                {
                    return Err(format!("size fields gave {} x {}", after.x, after.y).into());
                }
                println!("dimensions: {w} x {h} applied, {} x {}", after.x, after.y);
            }
            3 => {
                self.run(Action::Tool(shotoshot_core::Tool::Rectangle), event_loop);
                self.pointer_down(at(0.25, 0.3), event_loop);
                self.pointer_up(at(0.4, 0.45));
                self.run(Action::Tool(shotoshot_core::Tool::Arrow), event_loop);
                self.pointer_down(at(0.6, 0.55), event_loop);
                self.pointer_up(at(0.42, 0.4));
            }
            4 => {
                self.run(Action::Tool(shotoshot_core::Tool::Text), event_loop);
                self.pointer_down(at(0.45, 0.25), event_loop);
                if let Some(field) = &mut self.native.field {
                    field.insert("Shotoshot on Linux");
                }
                self.pointer_move(at(0.5, 0.5));
            }
            _ => {
                let out = self
                    .diagnostic
                    .as_ref()
                    .map(|d| d.out.clone())
                    .unwrap_or_default();
                let frame = shotoshot_raster::image_from_pixmap(&self.frame, 0, 0);
                png_io::save(&frame, &out.join("frame.png"))?;
                // The compositor's own view proves the window is really on screen.
                let grim = std::process::Command::new("grim")
                    .arg(out.join("screen.png"))
                    .status();
                println!(
                    "screen: {}",
                    if grim.is_ok_and(|s| s.success()) {
                        "grim ok"
                    } else {
                        "grim unavailable"
                    }
                );
                self.native.finish_text(&mut self.capture, true);
                let saved = self.save(Some(&out))?;
                println!(
                    "editor: {} objects, saved {}",
                    self.capture.editor.objects().len(),
                    saved.display()
                );
                // Another client must read the copy while the helper owns the clipboard.
                let expected = self.export()?;
                let mut helper = clipboard::copy_image(&expected)?;
                std::thread::sleep(std::time::Duration::from_millis(600));
                let read = clipboard::image_size();
                let _ = helper.kill();
                let _ = helper.wait();
                let (w, h) = read?;
                if (w, h) != (expected.width as usize, expected.height as usize) {
                    return Err(format!("clipboard holds {w}x{h}").into());
                }
                println!("clipboard: {w}x{h} read back");
                event_loop.exit();
                return Ok(());
            }
        }
        if let Some(d) = &mut self.diagnostic {
            d.step += 1;
        }
        self.redraw();
        Ok(())
    }
}

/// Typed text and clipboard work left after a key reached an inline field.
fn follow_up(
    handled: Option<FieldKey>,
    typed: Option<&str>,
    ctrl: bool,
    insert: &mut dyn FnMut(&str),
) -> Result<(), Box<dyn Error>> {
    match handled {
        Some(FieldKey::Ignored) | None if !ctrl => {
            if let Some(typed) = typed {
                insert(typed);
            }
        }
        Some(FieldKey::Copy(text)) if !text.is_empty() => {
            clipboard::copy_text(&text)?;
        }
        Some(FieldKey::Paste) => {
            if let Some(text) = clipboard::paste_text() {
                insert(&text);
            }
        }
        _ => {}
    }
    Ok(())
}

fn pictures_dir() -> PathBuf {
    user_dir("XDG_PICTURES_DIR", "Pictures")
}
/// An XDG user folder from `~/.config/user-dirs.dirs`, else `~/<fallback>`.
pub fn user_dir(key: &str, fallback: &str) -> PathBuf {
    let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
    let prefix = format!("{key}=");
    let configured = std::fs::read_to_string(home.join(".config/user-dirs.dirs"))
        .ok()
        .and_then(|dirs| {
            dirs.lines()
                .find_map(|l| l.strip_prefix(prefix.as_str()).map(str::to_owned))
                .map(|v| {
                    v.trim_matches('"')
                        .replace("$HOME", &home.to_string_lossy())
                })
        });
    configured.map_or_else(|| home.join(fallback), PathBuf::from)
}

impl ApplicationHandler for Editor {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("Shotoshot")
            .with_decorations(false)
            .with_fullscreen(Some(Fullscreen::Borderless(None)));
        let created = event_loop.create_window(attributes).map(Rc::new);
        let window = match created {
            Ok(window) => window,
            Err(error) => return self.fail(event_loop, error.into()),
        };
        let surface = softbuffer::Context::new(window.clone())
            .and_then(|context| softbuffer::Surface::new(&context, window.clone()));
        match surface {
            Ok(surface) => self.surface = Some(surface),
            Err(error) => return self.fail(event_loop, error.into()),
        }
        // With several monitors, edit only the one the window covers, at 1:1.
        let area = |m: MonitorHandle| monitors::Area {
            x: m.position().x,
            y: m.position().y,
            width: m.size().width,
            height: m.size().height,
        };
        let all: Vec<_> = event_loop.available_monitors().map(area).collect();
        let size = (self.capture.image.width, self.capture.image.height);
        if let Some(current) = window.current_monitor()
            && let Some(crop) = monitors::crop_for(size, &all, area(current))
            && let Some(image) = monitors::crop(&self.capture.image, crop)
        {
            match pixmap_from_bgra(&image) {
                Ok(backdrop) => {
                    self.frame = backdrop.clone();
                    self.backdrop = backdrop;
                    self.capture = prepared(image, &self.settings);
                }
                Err(error) => return self.fail(event_loop, error.0.into()),
            }
        }
        self.set_scale(window.scale_factor());
        window.set_cursor(CursorIcon::Crosshair);
        self.window = Some(window);
        self.redraw();
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.set_scale(scale_factor);
                self.redraw();
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.paint() {
                    return self.fail(event_loop, error);
                }
                if let Err(error) = self.diagnostic_step(event_loop) {
                    self.fail(event_loop, error);
                }
            }
            WindowEvent::ModifiersChanged(m) => {
                let state: ModifiersState = m.state();
                self.native.modifiers = Modifiers {
                    shift: state.shift_key(),
                    alt: state.alt_key(),
                    ctrl: state.control_key(),
                };
                let modifiers = self.native.modifiers;
                self.capture.editor.update_modifiers(modifiers);
                self.redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let p = self.to_capture(position.x, position.y);
                self.pointer_move(p);
                self.redraw();
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                let p = self.capture.pointer;
                match state {
                    ElementState::Pressed => self.pointer_down(p, event_loop),
                    ElementState::Released => self.pointer_up(p),
                }
                self.redraw();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y as f64,
                    MouseScrollDelta::PixelDelta(p) => p.y / 40.,
                };
                // Touchpads report fractions of a line; the capture sums them into notches.
                let travel = (lines * f64::from(wheel::NOTCH)).round() as i16;
                if travel != 0 {
                    if let Err(error) = self.capture.wheel(&mut self.native, travel) {
                        self.fail(event_loop, error);
                    }
                    self.redraw();
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let vk = keymap::virtual_key(&event.logical_key, event.physical_key);
                self.key(vk, event.text.as_deref(), event_loop);
                self.redraw();
            }
            _ => {}
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Wake only for hover-hint delays and fades; otherwise sleep until input.
        match self.capture.hover_hint.next_tick(Instant::now()) {
            Some(ms) => {
                event_loop.set_control_flow(ControlFlow::WaitUntil(
                    Instant::now() + std::time::Duration::from_millis(ms as u64),
                ));
                self.redraw();
            }
            None => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }
}
