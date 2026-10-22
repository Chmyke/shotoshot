//! ROLE: `Clear board` from the resident's menu: every note gone from every screen, present or
//! not, and the base alone back on the screens that show the board, in a process of its own.
//! INVARIANTS: Nothing is cleared while a board session is open: its notes would come back when
//! it ends. The document is written before any screen changes, under the bake lock, so no bake
//! reads the notes between their removal and their last picture.
use super::{bake, wallpaper::Wallpaper};
use crate::settings::{Settings, Store};
use shotoshot_core::board::Board;
use std::path::Path;
use windows::{
    Win32::{
        Foundation::{E_FAIL, ERROR_ALREADY_EXISTS, GetLastError},
        System::Threading::CreateMutexW,
    },
    core::{Error, Result, w},
};

/// Starts the clear in its own process so the resident keeps nothing.
pub fn spawn() -> Result<()> {
    let exe = std::env::current_exe().map_err(|e| Error::new(E_FAIL, e.to_string()))?;
    std::process::Command::new(exe)
        .arg("--board-clear")
        .spawn()
        .map(|_| ())
        .map_err(|e| Error::new(E_FAIL, e.to_string()))
}

/// `--board-clear`: the document emptied, then, when the board is shown, the base alone baked
/// on each screen. An open session is refused with a message: the user closes it first.
pub fn process() -> Result<()> {
    if session_open()? {
        return Err(crate::failure::visible(
            shotoshot_text::Key::ErrorBoardInUse,
            &[],
        ));
    }
    let folder = super::folder()?;
    // ORDER: the lock first, held through the bake; a session ending or a bake in another
    // process neither reads the old notes after this write nor writes its own picture over ours.
    let _lock = bake::Lock::acquire()?;
    clear_document(&folder)?;
    let visible = Settings::load()
        .map_err(crate::settings::unreadable)?
        .board_visible;
    if visible {
        bake::bake(&folder, &Wallpaper::new()?)?;
    }
    Ok(())
}

/// The board document without any note, for every screen, present or not.
pub fn clear_document(folder: &Path) -> Result<()> {
    crate::save_bytes(
        Board::default().encode().as_bytes(),
        &super::document_path(folder),
        false,
    )
    .map(|_| ())
}

/// Whether a board session holds its mutex in this logon session.
fn session_open() -> Result<bool> {
    unsafe {
        let mutex = CreateMutexW(None, false, w!("Local\\Shotoshot.Board"))?;
        let _mutex = crate::resources::OwnedMutex(mutex);
        Ok(GetLastError() == ERROR_ALREADY_EXISTS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shotoshot_core::{
        Object, Point, Rect, Shape, Style,
        board::{Gathered, Screen},
    };

    #[test]
    fn clearing_the_document_removes_the_notes_of_every_screen_present_or_not() {
        let dir = std::env::temp_dir().join(format!("shotoshot-clear-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let screen = |id: &str| Screen {
            id: id.into(),
            bounds: Rect::new(Point::default(), Point { x: 100., y: 50. }).unwrap(),
        };
        let note = Object {
            id: 1,
            rotation: 0.,
            shape: Shape::Rectangle(
                Rect::new(Point { x: 10., y: 10. }, Point { x: 40., y: 30. }).unwrap(),
            ),
            style: Style {
                color: [1, 2, 3],
                width: 2.,
            },
            fill: Vec::new(),
        };
        let mut board = Board::default();
        board.scatter(
            &Gathered {
                objects: vec![note.clone()],
                last_id: 1,
                background: vec![shotoshot_core::Paint::solid([9, 9, 9])],
            },
            &[screen("present")],
        );
        board.scatter(
            &Gathered {
                objects: vec![note],
                last_id: 1,
                background: Vec::new(),
            },
            &[screen("unplugged")],
        );
        assert_eq!(board.screens.len(), 2);
        std::fs::write(super::super::document_path(&dir), board.encode()).unwrap();

        clear_document(&dir).unwrap();
        let read = bake::read_board(&dir).unwrap();
        assert_eq!(read, Board::default(), "no screen keeps a note");
        assert!(
            read.gather(&[screen("present"), screen("unplugged")])
                .objects
                .is_empty()
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_session_holding_its_mutex_is_seen_as_open() {
        // The positive case only: a real session on this desktop would also hold the mutex.
        let held = unsafe { CreateMutexW(None, false, w!("Local\\Shotoshot.Board")).unwrap() };
        let _held = crate::resources::OwnedMutex(held);
        assert!(session_open().unwrap());
    }
}
