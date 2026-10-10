//! ROLE: The `--board` process: opens the notes in the editor over the desktop picture, writes
//! and bakes them when the session ends. The resident only starts this process.
//! INVARIANTS: A board session ends only once its notes are written; the bake is tried before
//! the window hides.
use super::*;
use crate::board::{bake, base::Base, rendering, wallpaper::Wallpaper};
use shotoshot_core::board::{Gathered, Screen};
use std::path::Path;

/// Starts a board session in its own process; a second one is refused by that process' mutex.
pub(crate) fn spawn_session() -> Result<()> {
    let exe = std::env::current_exe().map_err(|e| Error::new(E_FAIL, e.to_string()))?;
    let child = std::process::Command::new(exe)
        .arg("--board")
        .spawn()
        .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
    // The resident got the shortcut or the menu: it may hand the foreground to the editor.
    unsafe {
        let _ = AllowSetForegroundWindow(child.id());
    }
    Ok(())
}

/// The resident's decision once the wallpaper or the displays settled: a shown board is baked
/// again when the displays changed (`forced`) or when some screen no longer shows our picture.
/// `all_ours` reads the desktop, only when that decides.
pub(super) fn rebake_needed(
    visible: bool,
    forced: bool,
    all_ours: impl FnOnce() -> Result<bool>,
) -> Result<bool> {
    Ok(visible && (forced || !all_ours()?))
}
/// Whether every present screen shows its baked picture. With no base, nothing was baked here
/// and there is nothing to put back.
pub(super) fn desktop_shows_ours() -> Result<bool> {
    let wallpaper = Wallpaper::new()?;
    match Base::read(&crate::board::folder()?)? {
        Some(base) => base.all_ours(&wallpaper, &wallpaper.screens()?),
        None => Ok(true),
    }
}

/// The session's notes written back into the board file (desktop coordinates), each to the
/// one of `screens` that holds it; the other screens keep theirs.
pub(super) fn write_document(folder: &Path, session: &Session, screens: &[Screen]) -> Result<()> {
    let image = &session.capture.image;
    let shift = Point {
        x: image.left as f64,
        y: image.top as f64,
    };
    let gathered = Gathered {
        objects: session
            .capture
            .editor
            .objects()
            .iter()
            .cloned()
            .map(|mut o| {
                o.translate(shift);
                o
            })
            .collect(),
        last_id: session.capture.editor.last_id(),
        background: session.capture.editor.background(),
    };
    let mut board = bake::read_board(folder)?;
    board.scatter(&gathered, screens);
    crate::save_bytes(
        board.encode().as_bytes(),
        &crate::board::document_path(folder),
        false,
    )
    .map(|_| ())
}

impl App {
    /// Opens the board: the desktop picture (base and icons, without our baked notes) as the
    /// frame, the notes over it.
    pub(super) fn open_board(&mut self, hwnd: HWND) -> Result<()> {
        let folder = crate::board::folder()?;
        let wallpaper = Wallpaper::new()?;
        let (screens, frame) = {
            // ORDER: a bake in another process must not set the wallpaper or write the base
            // between what is read here and what is written.
            let _lock = bake::Lock::acquire()?;
            let screens = wallpaper.screens()?;
            let mut base = match Base::read(&folder)? {
                Some(base) => base,
                None => Base::capture(&wallpaper, &screens)?,
            };
            base.adopt(&wallpaper, &screens)?;
            base.write(&folder)?;
            let base_frame = bake::frame(&base, &screens)?;
            let desktop = rendering::desktop_frame(&base_frame, &wallpaper, &screens)?;
            (screens, desktop)
        };
        let rendering::DesktopFrame { frame, icons } = frame;
        let mut gathered = bake::read_board(&folder)?.gather(&screens);
        let shift = Point {
            x: -(frame.left as f64),
            y: -(frame.top as f64),
        };
        for object in &mut gathered.objects {
            object.translate(shift);
        }
        self.enter(hwnd, Session::board(frame, icons, gathered, screens)?)
    }
    /// Writes the notes, the text being typed included, to the screens the session opened
    /// on. Until this succeeds the session must stay open.
    pub(super) fn save_board(&mut self, hwnd: HWND) -> Result<()> {
        let Some(session) = &mut self.session else {
            return Ok(());
        };
        let Some(screens) = &session.board else {
            return Ok(());
        };
        session.native.finish_text(&mut session.capture, hwnd, true);
        write_document(&crate::board::folder()?, session, screens)
    }
    /// Bakes the saved notes, sets the wallpaper and remembers that the board is shown. Returns
    /// once Windows shows the new pictures at the pixels the bake changed (at least 95 % of up to
    /// 1024 sampled ones per screen, see `rendering::wait_until_shown`), so the window that hides
    /// next uncovers the notes as they were left rather than the previous ones. Not guaranteed
    /// past `SHOWN_WAIT` (1.5 s) or when the desktop cannot be read: it then returns anyway.
    pub(super) fn apply_board(&self) -> Result<()> {
        let folder = crate::board::folder()?;
        let wallpaper = Wallpaper::new()?;
        // ORDER: before the bake rewrites the pictures of ours that the screens show. Unread,
        // the wait takes every screen for its base; the bake itself does not depend on it.
        let before = rendering::shown_pictures(&wallpaper).unwrap_or_default();
        let baked = bake::run(&folder, &wallpaper)?;
        if self.persist_preferences {
            Settings::save_board_visible(true).map_err(|e| Error::new(E_FAIL, e))?;
        }
        rendering::wait_until_shown(&before, &baked, rendering::SHOWN_WAIT);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shotoshot_core::Rect;

    #[test]
    fn the_resident_bakes_again_only_a_shown_board_that_needs_it() {
        let ours = || Ok(true);
        let foreign = || Ok(false);
        let unread = || -> Result<bool> { panic!("the desktop was read for nothing") };
        assert!(
            rebake_needed(true, false, foreign).unwrap(),
            "another picture"
        );
        assert!(!rebake_needed(true, false, ours).unwrap(), "our pictures");
        assert!(
            rebake_needed(true, true, unread).unwrap(),
            "the displays changed"
        );
        assert!(!rebake_needed(false, false, unread).unwrap(), "hidden");
        assert!(
            !rebake_needed(false, true, unread).unwrap(),
            "hidden, displays changed"
        );
        let failed = || Err(Error::new(E_FAIL, "no desktop"));
        assert!(rebake_needed(true, false, failed).is_err());
    }

    #[test]
    fn closing_a_board_session_writes_the_document() {
        let _com = crate::ComApartment::new().unwrap();
        let folder =
            std::env::temp_dir().join(format!("shotoshot-board-close-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let screens = vec![shotoshot_core::board::Screen {
            id: "test".into(),
            bounds: Rect::new(Point { x: -100., y: 0. }, Point { x: 200., y: 100. }).unwrap(),
        }];
        let mut frame = crate::Image::solid(300, 100, [0, 0, 0, 255]);
        frame.left = -100;
        let mut session =
            Session::board(frame, None, Gathered::default(), screens.clone()).unwrap();
        session.capture.editor.tool = shotoshot_core::Tool::Rectangle;
        session
            .capture
            .editor
            .begin(Point { x: 20., y: 20. }, Default::default());
        session.capture.editor.update(Point { x: 80., y: 60. });
        session.capture.editor.finish();
        // The part of save_board that needs no desktop: the document.
        write_document(&folder, &session, &screens).unwrap();
        let board = crate::board::bake::read_board(&folder).unwrap();
        assert_eq!(board.screens[0].scene.objects.len(), 1);
        // Frame (20, 20) is desktop (-80, 20): the frame starts 100 px left of the origin.
        let gathered = board.gather(&screens);
        assert_eq!(gathered.objects[0].bounds().min, Point { x: -80., y: 20. });
        std::fs::remove_dir_all(folder).unwrap();
    }
}
