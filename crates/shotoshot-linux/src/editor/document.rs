//! ROLE: A `.shoto` document in the Linux editor: read from its file, placed on today's
//! monitors, written back, and proved on a real desktop by the diagnostic.
//! OWNER: A process of its own, started by the desktop for the file; no resident is involved.
//! INVARIANTS: The frame of a document is decoded only once a monitor is known to show it
//! whole, and only as the PNG of the announced size.
use super::*;
use crate::failure::visible;
use shotoshot_core::{
    Rect, Tool,
    shoto::{self, Scene},
};
use winit::dpi::PhysicalSize;

/// Second argument of the diagnostic's children: stand in for the user, then leave.
pub const PROBE: &str = "--linux-document-probe";

/// A reopened document.
pub(super) struct Document {
    /// Save rewrites this file.
    pub(super) path: PathBuf,
    /// Its scene and the PNG of its frame, until `resumed` knows the monitors.
    unopened: Option<(Scene, Vec<u8>)>,
}

/// Where the window of a reopened frame goes.
pub(super) enum Place {
    /// Full screen on a monitor of exactly its size, as at capture time.
    Monitor(MonitorHandle),
    /// A window of its size on a larger monitor; the desktop chooses where.
    Window(PhysicalSize<u32>),
}

/// Edits the document at `path`, as the capture it was saved from. `probe` runs the
/// scripted user of the diagnostic.
pub fn open(path: &Path, probe: bool) -> Result<(), Box<dyn Error>> {
    // A file that does not open says so in the stored language, before any editor starts; the
    // scripted user stays in English, as every diagnostic.
    if !probe {
        crate::language::apply(&settings::load().unwrap_or_default());
    }
    let unread = |error: std::io::Error| {
        let path = path.display().to_string();
        let detail = error.to_string();
        visible(Text::with(
            Key::ErrorDocumentUnread,
            &[("path", &path), ("detail", &detail)],
        ))
    };
    if std::fs::metadata(path).map_err(unread)?.len() > shoto::MAX_FILE {
        return Err(visible(shoto::Error::Damaged.message()).into());
    }
    let file = std::fs::read(path).map_err(unread)?;
    let (scene, frame) = shoto::unpack(&file).map_err(|e| visible(e.message()))?;
    let document = Document {
        path: path.into(),
        unopened: Some((scene, frame.to_vec())),
    };
    let stand_in = Image::solid(1, 1, [0, 0, 0, 255]);
    start(
        stand_in,
        Some(document),
        probe.then_some(Diagnostic::Document),
    )
}

/// The `.shoto` file of `capture`: its flattened image as any viewer shows it, and inside
/// it the frame and the objects, still apart.
pub(super) fn packed(raster: &mut Raster, capture: &Capture) -> Result<Vec<u8>, Box<dyn Error>> {
    let visible = png_io::encode(&flattened(raster, capture)?)?;
    let refused = || crate::failure::visible(Text::new(Key::ErrorNotEditableCapture));
    let (scene, frame) = capture.document(&capture.image).ok_or_else(refused)?;
    Ok(shoto::pack(&visible, &scene, &png_io::encode(&frame)?).ok_or_else(refused)?)
}

/// What the scripted user does to a reopened document, and the diagnostic to its copy.
pub(super) fn move_first_object(capture: &mut Capture) -> Result<(), Box<dyn Error>> {
    let first = capture.editor.objects().first().ok_or("no object")?.id;
    capture.editor.select_only(first);
    capture.editor.nudge(Point { x: 1., y: 0. });
    Ok(())
}

/// A plain screen of `width` × `height` with a selection and one rectangle on it.
fn sample(width: u32, height: u32) -> Capture {
    let corner = |x, y| Point { x, y };
    let mut capture = Capture::new(Image::solid(width, height, [90, 60, 30, 255]));
    capture.region = Some(Rect {
        min: corner(8., 8.),
        max: corner(200., 120.),
    });
    capture.selecting = false;
    capture.editor.tool = Tool::Rectangle;
    capture.editor.begin(corner(20., 20.), Modifiers::default());
    capture.editor.update(corner(90., 70.));
    capture.editor.finish();
    capture
}

/// The scripted user on the document at `path`, through the real launch path.
fn probe(path: &Path) -> std::io::Result<std::process::Output> {
    std::process::Command::new(std::env::current_exe()?)
        .arg(path)
        .arg(PROBE)
        .output()
}

/// Whether the file at `path` holds `capture` as edited by the scripted user.
fn holds_edited(path: &Path, capture: &mut Capture) -> Result<bool, Box<dyn Error>> {
    move_first_object(capture)?;
    let file = std::fs::read(path)?;
    let (scene, frame) = shoto::unpack(&file).map_err(shoto::Error::message)?;
    let image = &capture.image;
    Ok(scene.objects == capture.editor.objects()
        && Some(scene.region) == capture.region
        && png_io::decode(frame, (image.width, image.height))?.pixels == image.pixels)
}

impl Editor {
    /// Decodes the document waiting to be opened, as the capture it was saved from, and
    /// says where its window goes. `None` for a fresh capture.
    pub(super) fn open_document(
        &mut self,
        event_loop: &ActiveEventLoop,
    ) -> Result<Option<Place>, Box<dyn Error>> {
        let Some((scene, frame)) = self.document.as_mut().and_then(|d| d.unopened.take()) else {
            return Ok(None);
        };
        let size = (scene.width, scene.height);
        let handles: Vec<MonitorHandle> = event_loop.available_monitors().collect();
        let areas: Vec<_> = handles.iter().map(area).collect();
        let (corner, filled) = monitors::place(size, scene.origin, &areas).ok_or_else(|| {
            let (width, height) = (size.0.to_string(), size.1.to_string());
            visible(Text::with(
                Key::ErrorDocumentTooLarge,
                &[("width", &width), ("height", &height)],
            ))
        })?;
        if matches!(self.diagnostic, Some(Diagnostic::Document)) {
            let shown = if filled.is_some() {
                "monitor"
            } else {
                "window"
            };
            println!("placement: {shown}");
        }
        let place = match filled {
            Some(monitor) => Place::Monitor(handles[monitor].clone()),
            None => Place::Window(PhysicalSize::new(size.0, size.1)),
        };
        let damaged = || visible(shoto::Error::Damaged.message());
        // Opaque whatever the file says: the decoder keeps no alpha.
        let mut frame = png_io::decode(&frame, size).map_err(|_| damaged())?;
        (frame.left, frame.top) = corner;
        let raster = &mut self.native.raster;
        // The font is not in the file: text is measured again with the one drawn here.
        let measure = |text: &str, size: f64, label: Option<shotoshot_core::Label>| {
            Some(match label.and_then(shotoshot_core::Label::wrap) {
                Some(width) => raster.measure_wrapped_annotation_text(text, size, width),
                None => raster.measure_annotation_text(text, size),
            })
        };
        let capture = Capture::reopen(scene, frame, Vec::new(), measure).ok_or_else(damaged)?;
        self.show(capture)?;
        Ok(Some(place))
    }
    /// Proves documents on the real desktop, each through the launch path of a double-click:
    /// this capture saved and reopened full screen, a smaller frame reopened in a window,
    /// and a frame larger than every monitor refused.
    pub(super) fn document_diagnostic(
        &mut self,
        out: &Path,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn Error>> {
        let said = |output: &std::process::Output, placement: &str| {
            output.status.success() && String::from_utf8_lossy(&output.stdout).contains(placement)
        };
        let (path, _) = self.save(true, Some(out))?;
        if !said(&probe(&path)?, "placement: monitor") {
            return Err("the reopened document did not fill its monitor, save and leave".into());
        }
        if !holds_edited(&path, &mut self.capture)? {
            return Err("the saved document does not hold the edited capture".into());
        }
        if png_io::load(&path)?.pixels != self.export()?.pixels {
            return Err("a viewer would not show the edited capture".into());
        }
        println!(
            "document: {} of {} bytes reopened by its launch path, edited, saved and read back",
            path.file_name().unwrap_or_default().to_string_lossy(),
            std::fs::metadata(&path)?.len()
        );

        let image = &self.capture.image;
        let mut small = sample(image.width / 2, image.height / 2);
        let window = out.join("window.shoto");
        files::replace(&packed(&mut self.native.raster, &small)?, &window)?;
        if !said(&probe(&window)?, "placement: window") || !holds_edited(&window, &mut small)? {
            return Err("a frame smaller than the monitor did not reopen in a window".into());
        }

        let widest = event_loop
            .available_monitors()
            .map(|m| m.size().width)
            .max()
            .ok_or("no monitor")?;
        let large = sample(widest + 16, image.height);
        let refused = out.join("too-large.shoto");
        let bytes = packed(&mut self.native.raster, &large)?;
        files::replace(&bytes, &refused)?;
        let output = probe(&refused)?;
        if output.status.success()
            || !String::from_utf8_lossy(&output.stderr).contains("larger than this screen")
            || std::fs::read(&refused)? != bytes
        {
            return Err("a frame larger than every monitor was not refused untouched".into());
        }
        println!(
            "document: {}x{} reopened in a window, {}x{} refused",
            small.image.width, small.image.height, large.image.width, large.image.height
        );
        Ok(())
    }
}
