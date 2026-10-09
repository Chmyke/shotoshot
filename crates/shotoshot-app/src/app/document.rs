//! ROLE: Reopening a saved `.shoto` document: the launch argument, the session it starts,
//! and the probe that proves the whole path on the built binary.
//! OWNER: A process of its own, started by Windows for the file; no resident is involved.
use super::*;
use shotoshot_core::{Modifiers, Rect, Tool, shoto};
use std::path::{Path, PathBuf};

/// Second argument of the probe's child: stand in for the user, then leave.
const PROBE: &str = "--document-probe";
const REPORT: &str = "target/document-diagnostics.txt";

/// The document to reopen, when the first argument is a file rather than a switch: what
/// Windows passes for a double-click on a `.shoto` file.
pub(super) fn requested() -> Option<PathBuf> {
    let first = std::env::args_os().nth(1)?;
    (!first.to_string_lossy().starts_with("--")).then(|| first.into())
}

impl App {
    /// Starts a session on the document at `path` instead of a fresh capture.
    pub(super) fn open_document(&mut self, hwnd: HWND, path: &Path) -> Result<()> {
        let unread = |error: std::io::Error| {
            Error::new(
                E_FAIL,
                format!("Could not open {}: {error}", path.display()),
            )
        };
        let refused = |error: shoto::Error| Error::new(E_FAIL, error.message());
        if std::fs::metadata(path).map_err(unread)?.len() > shoto::MAX_FILE {
            return Err(refused(shoto::Error::Damaged));
        }
        let file = std::fs::read(path).map_err(unread)?;
        let (scene, frame) = shoto::unpack(&file).map_err(refused)?;
        self.enter(hwnd, Session::reopen(scene, frame, path)?)?;
        if std::env::args().any(|a| a == PROBE) {
            if let Some(s) = &mut self.session
                && let Some(first) = s.capture.editor.objects().first().map(|o| o.id)
            {
                s.capture.editor.select_only(first);
                s.capture.editor.nudge(Point { x: 1., y: 0. });
            }
            self.action(hwnd, Action::Save)?;
        }
        Ok(())
    }
}

/// `--document-diagnostic`: saves a document, has the real launch path reopen it, move its
/// first object and save it, then checks what the file holds and what a viewer shows.
pub(super) fn probe() -> Result<()> {
    let fail = |text: &str| Error::new(E_FAIL, text);
    let io = |error: std::io::Error| Error::new(E_FAIL, error.to_string());
    let path = std::env::current_dir()
        .map_err(io)?
        .join("target")
        .join("shotoshot-document.shoto");
    let mut image = crate::Image::solid(320, 200, [0, 0, 0, 255]);
    for (i, pixel) in image.pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        (pixel[0], pixel[1], pixel[2]) = ((i % 320) as u8, (i / 320) as u8, 90);
    }
    let mut session = Session::new(image)?;
    let corner = |x, y| Point { x, y };
    let region = Rect::new(corner(40., 30.), corner(280., 170.));
    session.capture.region = region;
    session.capture.selecting = false;
    let editor = &mut session.capture.editor;
    editor.tool = Tool::Rectangle;
    editor.begin(corner(60., 50.), Modifiers::default());
    editor.update(corner(140., 110.));
    editor.finish();
    let bytes = session.document_bytes(&session.capture.image)?;
    crate::save_bytes(&bytes, &path, false)?;

    let child = std::process::Command::new(std::env::current_exe().map_err(io)?)
        .arg(&path)
        .arg(PROBE)
        .status()
        .map_err(io)?;
    if !child.success() {
        return Err(fail("The reopened document did not save and leave"));
    }

    // What the user did in the reopened document, done here to the original.
    let editor = &mut session.capture.editor;
    editor.select_only(editor.objects()[0].id);
    editor.nudge(corner(1., 0.));
    let file = std::fs::read(&path).map_err(io)?;
    let (scene, frame) = shoto::unpack(&file).map_err(|error| fail(error.message()))?;
    if scene.objects != session.capture.editor.objects() || Some(scene.region) != region {
        return Err(fail("The saved document does not hold the edited objects"));
    }
    if crate::decode_png(frame, (320, 200))?.pixels != session.capture.image.pixels {
        return Err(fail("The saved document does not hold the original screen"));
    }
    if crate::load_png(&path)?.pixels != session.export()?.pixels {
        return Err(fail("A viewer would not show the edited capture"));
    }
    std::fs::write(
        REPORT,
        format!(
            "PASS: a {} byte document reopened by its launch path, edited, saved and read back.",
            file.len()
        ),
    )
    .map_err(io)
}
