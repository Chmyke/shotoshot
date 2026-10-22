//! ROLE: Bakes the board into one picture per screen and sets it as that screen's wallpaper;
//! gives the user's base back when the board is hidden.
//! INVARIANTS: Pictures are written atomically before any screen is set; the resident never
//! calls `run` directly, a short-lived process does (`spawn`), and only `hide`, which is quick
//! (it waits for the shell to apply the base, 2 s at most), runs on its thread; `run` and `hide`
//! never overlap, in this process or another of the same logon session.
use super::{
    base::{self, Base, same_picture},
    wallpaper::{Position, Wallpaper},
};
use crate::{
    Image,
    render::{Backend, Renderer},
    resources::OwnedMutex,
    settings::{Settings, Store},
};
use shotoshot_core::{
    Point, Rect,
    board::{Board, Gathered, Screen},
};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{E_FAIL, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT},
        System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject},
    },
    core::{Error, PCWSTR, Result, w},
};

/// How long a bake asked for by the user (a session ending, the menu) waits for another one to
/// finish before giving up with an error.
const LOCK_WAIT_MS: u32 = 10_000;
/// How long a `--board-bake` process waits for its turn before leaving quietly: a storm of
/// display changes may queue several, and the one holding the lock bakes the latest state.
const BACKGROUND_LOCK_WAIT_MS: u32 = 60_000;
/// How often `hide` looks at what the desktop shows, and for how long before it gives up.
const SETTLE_POLL: Duration = Duration::from_millis(250);
const SETTLE_TIMEOUT: Duration = Duration::from_secs(2);

fn lock_name() -> PCWSTR {
    w!("Local\\Shotoshot.BoardBake")
}

/// Holds the bake mutex of this logon session (`Local\`): one bake or hide at a time,
/// whichever process runs it, so `base.txt` and the wallpaper are never written by two at once.
pub(crate) struct Lock(OwnedMutex);
impl Lock {
    pub(crate) fn acquire() -> Result<Self> {
        Self::acquire_as(lock_name(), LOCK_WAIT_MS)
    }
    fn acquire_as(name: PCWSTR, wait_ms: u32) -> Result<Self> {
        Self::wait(name, wait_ms)?
            .ok_or_else(|| Error::new(E_FAIL, "Another bake is still running"))
    }
    /// The lock, or `None` once another holder kept it for `wait_ms`.
    fn wait(name: PCWSTR, wait_ms: u32) -> Result<Option<Self>> {
        let mutex = OwnedMutex(unsafe { CreateMutexW(None, false, name)? });
        // An abandoned mutex belongs to us now: its owner died mid-bake, and every file it
        // writes is replaced whole, so the next bake starts from a consistent state.
        match unsafe { WaitForSingleObject(mutex.0, wait_ms) } {
            WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(Some(Self(mutex))),
            WAIT_TIMEOUT => Ok(None),
            _ => Err(Error::from_thread()),
        }
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        // ORDER: released here, on the owning thread, before `OwnedMutex` closes the handle.
        unsafe {
            let _ = ReleaseMutex(self.0.0);
        }
    }
}

/// The bounding rectangle of every screen, in desktop coordinates.
pub fn desktop(screens: &[Screen]) -> Rect {
    screens
        .iter()
        .map(|s| s.bounds)
        .reduce(|a, b| a.union(b))
        .unwrap_or(Rect {
            min: Point::default(),
            max: Point { x: 1., y: 1. },
        })
}
/// The base of the whole virtual desktop: each screen's own base at its place, `left`/`top` those
/// of the union.
pub fn frame(base: &Base, screens: &[Screen]) -> Result<Image> {
    let whole = desktop(screens);
    let size = whole.size();
    let mut frame = Image::solid(size.x as u32, size.y as u32, [0, 0, 0, 255]);
    (frame.left, frame.top) = (whole.min.x as i32, whole.min.y as i32);
    for screen in screens {
        let size = screen.bounds.size();
        let fallback = base::ScreenBase {
            id: screen.id.clone(),
            image: None,
            baked: None,
        };
        let picture = base::render(
            base.screen(&screen.id).unwrap_or(&fallback),
            base.position,
            base.color,
            (size.x as u32, size.y as u32),
        )?;
        let (x, y) = (
            screen.bounds.min.x as i32 - frame.left,
            screen.bounds.min.y as i32 - frame.top,
        );
        base::blit(&mut frame, &picture, x, y);
    }
    Ok(frame)
}
/// One picture per screen: the frame plus every note, rendered once and cut per screen.
pub fn render_screens(
    renderer: &Renderer,
    frame: &Image,
    gathered: &Gathered,
    screens: &[Screen],
) -> Result<Vec<(String, Image)>> {
    let shift = Point {
        x: -(frame.left as f64),
        y: -(frame.top as f64),
    };
    let objects: Vec<_> = gathered
        .objects
        .iter()
        .cloned()
        .map(|mut o| {
            o.translate(shift);
            o
        })
        .collect();
    let mut pictures = Vec::new();
    for screen in screens {
        let crop = Rect {
            min: screen.bounds.min + shift,
            max: screen.bounds.max + shift,
        };
        pictures.push((
            screen.id.clone(),
            renderer.export(frame, &gathered.background, &objects, crop)?,
        ));
    }
    Ok(pictures)
}
/// The saved board; none yet is an empty one.
pub fn read_board(folder: &Path) -> Result<Board> {
    match std::fs::read_to_string(super::document_path(folder)) {
        Ok(text) => Board::decode(&text).map_err(|e| crate::failure::visible(e.key(), &[])),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Board::default()),
        Err(e) => Err(Error::new(E_FAIL, e.to_string())),
    }
}
/// `path` made absolute against the current directory, without touching the disk: the
/// wallpaper API rejects a relative picture path, and `canonicalize` would add a `\\?\` prefix.
fn absolute(path: &Path) -> Result<PathBuf> {
    std::path::absolute(path).map_err(|e| Error::new(E_FAIL, e.to_string()))
}
/// What a bake set: the present screens, the base frame it drew on and each screen's picture.
pub struct Baked {
    pub screens: Vec<Screen>,
    pub frame: Image,
    pub pictures: Vec<(String, Image)>,
}
/// Bakes the saved board over the user's base and sets one picture per present screen.
pub fn run(folder: &Path, wallpaper: &Wallpaper) -> Result<Baked> {
    let _lock = Lock::acquire()?;
    bake(folder, wallpaper)
}
/// `run` once the lock is held.
pub(super) fn bake(folder: &Path, wallpaper: &Wallpaper) -> Result<Baked> {
    let screens = wallpaper.screens()?;
    if screens.is_empty() {
        return Err(Error::new(E_FAIL, "No screen"));
    }
    let board = read_board(folder)?;
    let mut base = match Base::read(folder)? {
        Some(base) => base,
        None => Base::capture(wallpaper, &screens)?,
    };
    // ORDER: before our pictures are set; once they are, the desktop's position is our own Fill.
    base.adopt(wallpaper, &screens)?;
    let frame = frame(&base, &screens)?;
    let renderer = Renderer::with(Backend::Software, false)?;
    let pictures = render_screens(&renderer, &frame, &board.gather(&screens), &screens)?;
    let mut paths = Vec::new();
    for (id, picture) in &pictures {
        // ABSOLUTE: this path is recorded in the base and handed to the wallpaper API.
        let path = absolute(&folder.join(format!("baked-{}.png", super::slug(id))))?;
        crate::save_png(picture, &path)?;
        base.set_baked(id, path.clone());
        paths.push((id.clone(), path));
    }
    base.write(folder)?;
    wallpaper.set_position(Position::Fill)?;
    for (id, path) in &paths {
        wallpaper.set(id, path)?;
    }
    Ok(Baked {
        screens,
        frame,
        pictures,
    })
}
/// What `hide` gives back to the present screens.
struct Restore<'a> {
    /// The base picture of every screen that has one whose file is still there.
    pictures: Vec<(&'a str, &'a Path)>,
    /// Whether the desktop must first go back to its solid colour: some screen has no picture
    /// to get back (a colour, a screen the base never knew, a picture deleted since).
    clear: bool,
}
fn what_to_restore<'a>(
    base: &'a Base,
    screens: &'a [Screen],
    exists: impl Fn(&Path) -> bool,
) -> Restore<'a> {
    let mut pictures = Vec::new();
    for screen in screens {
        let shown = base
            .screen(&screen.id)
            .and_then(|b| b.image.as_deref())
            .filter(|path| exists(path));
        if let Some(path) = shown {
            pictures.push((screen.id.as_str(), path));
        }
    }
    Restore {
        clear: pictures.len() < screens.len(),
        pictures,
    }
}
/// What a look at the desktop found wrong once `hide` has given the base back.
#[derive(Debug, Default, PartialEq)]
struct Unsettled<'a> {
    /// A screen that should show its solid colour still shows a picture: the global clear goes
    /// out again, and it wipes every picture, so then all of the base's are set again too.
    clear: bool,
    /// The base pictures to set again.
    pictures: Vec<(&'a str, &'a Path)>,
}
impl Unsettled<'_> {
    fn is_empty(&self) -> bool {
        !self.clear && self.pictures.is_empty()
    }
}
/// What is not right yet, given what each screen should show (`None` for its solid colour) and
/// what it shows, in the same order.
fn unsettled<'a>(
    expected: &[(&'a str, Option<&'a Path>)],
    shown: &[Option<PathBuf>],
) -> Unsettled<'a> {
    debug_assert_eq!(expected.len(), shown.len());
    let wrong: Vec<bool> = expected
        .iter()
        .zip(shown)
        .map(|(&(_, want), now)| !same_picture(&want.map(Path::to_path_buf), now))
        .collect();
    let clear = expected
        .iter()
        .zip(&wrong)
        .any(|(&(_, want), &wrong)| wrong && want.is_none());
    let pictures = expected
        .iter()
        .zip(&wrong)
        .filter(|&(_, &wrong)| clear || wrong)
        .filter_map(|(&(id, want), _)| want.map(|path| (id, path)))
        .collect();
    Unsettled { clear, pictures }
}
/// Keeps the first error of steps that are all tried.
fn keep_first(first: &mut Result<()>, step: Result<()>) {
    if first.is_ok() {
        *first = step;
    }
}
/// Looks every `poll` at what the desktop shows (`look`, one entry per expected screen) and
/// has `fix` issue what is missing again, until it all is right (`Ok`, whatever failed on the way
/// and was repaired) or `timeout` is over: then the first error met, `first` included, or one of
/// its own.
fn settle<'a>(
    expected: &[(&'a str, Option<&'a Path>)],
    mut first: Result<()>,
    mut look: impl FnMut() -> Result<Vec<Option<PathBuf>>>,
    mut fix: impl FnMut(&Unsettled<'a>) -> Result<()>,
    poll: Duration,
    timeout: Duration,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        // The shell applies a wallpaper in its own time: give it a beat before judging it.
        std::thread::sleep(poll);
        match look() {
            Ok(shown) => {
                let wrong = unsettled(expected, &shown);
                if wrong.is_empty() {
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    break;
                }
                keep_first(&mut first, fix(&wrong));
            }
            Err(error) => {
                keep_first(&mut first, Err(error));
                if Instant::now() >= deadline {
                    break;
                }
            }
        }
    }
    first.and(Err(Error::new(
        E_FAIL,
        "The desktop did not take the base back",
    )))
}
/// Gives each present screen its own base back; the board stays on disk. Returns once the
/// desktop shows it, or after 2 s with the first error.
pub fn hide(folder: &Path, wallpaper: &Wallpaper) -> Result<()> {
    let _lock = Lock::acquire()?;
    let Some(base) = Base::read(folder)? else {
        return Ok(());
    };
    give_back(&base, &wallpaper.screens()?, wallpaper)
}
/// `hide` once the lock is held and the base read.
fn give_back(base: &Base, screens: &[Screen], wallpaper: &Wallpaper) -> Result<()> {
    let restore = what_to_restore(base, screens, |path| path.is_file());
    // A step that fails leaves neither the other screens nor the position undone: the first
    // error is returned once everything has been tried.
    let mut first = Ok(());
    // PITFALL: the shell applies wallpapers asynchronously (260 to 617 ms measured), and a global
    // clear issued while a per-monitor `SetWallpaper` is still pending is lost: the pending
    // picture lands after it and the desktop keeps ours (observed 2026-10-10 on Windows 11 26200:
    // a `hide` right after a bake returned Ok and left the baked pictures, the same clear a little
    // later worked). The earlier explanation, `SetPosition` after the clear re-applying remembered
    // pictures, was not established; the evidence now points to timing, not order. The position
    // still goes first, which costs nothing. Hence `settle`: look, and issue what is missing again.
    keep_first(&mut first, wallpaper.set_position(base.position));
    if restore.clear {
        keep_first(&mut first, wallpaper.clear_all());
    }
    for &(id, path) in &restore.pictures {
        keep_first(&mut first, wallpaper.set(id, path));
    }
    let expected: Vec<_> = screens
        .iter()
        .map(|screen| {
            let id = screen.id.as_str();
            let want = restore.pictures.iter().find(|p| p.0 == id).map(|p| p.1);
            (id, want)
        })
        .collect();
    settle(
        &expected,
        first,
        || screens.iter().map(|s| wallpaper.shown(&s.id)).collect(),
        |wrong| {
            let mut failed = Ok(());
            if wrong.clear {
                keep_first(&mut failed, wallpaper.clear_all());
            }
            for &(id, path) in &wrong.pictures {
                keep_first(&mut failed, wallpaper.set(id, path));
            }
            failed
        },
        SETTLE_POLL,
        SETTLE_TIMEOUT,
    )
}
/// The base to give back to a desktop nobody watched since the last bake, or `None` when no
/// present screen shows the board any more: that desktop is the user's and stays as it is.
/// `shown` is what each present screen displays; a picture of the user's set since becomes the
/// base of its screen, with the desktop's `position` and `color` (`Base::adopt_desktop`).
fn base_to_give_back(
    mut base: Base,
    shown: &[(String, Option<PathBuf>)],
    position: Position,
    color: [u8; 3],
) -> Option<Base> {
    let board = shown.iter().any(|(id, picture)| {
        base.shows_ours(id, picture) || picture.as_deref().is_some_and(base::is_baked_name)
    });
    if !board {
        return None;
    }
    base.adopt_desktop(shown, position, color);
    Some(base)
}
/// `--restore-wallpaper`, run by the uninstaller: the base back on a desktop that still shows
/// the board, and nothing else. The board, its base and the settings stay on disk, so the notes
/// show again once Shotoshot is installed again.
pub fn restore() -> Result<()> {
    // Not `folder`, which creates it: without a folder there was never a board to take away.
    let folder = super::location()?;
    let _lock = Lock::acquire()?;
    let Some(base) = Base::read(&folder)? else {
        return Ok(());
    };
    let wallpaper = Wallpaper::new()?;
    let screens = wallpaper.screens()?;
    let shown = base::shown(&wallpaper, &screens)?;
    match base_to_give_back(base, &shown, wallpaper.position()?, wallpaper.color()?) {
        Some(base) => give_back(&base, &screens, &wallpaper),
        None => Ok(()),
    }
}
/// Starts a bake in its own process so the resident keeps nothing.
pub fn spawn() -> Result<()> {
    let exe = std::env::current_exe().map_err(|e| Error::new(E_FAIL, e.to_string()))?;
    std::process::Command::new(exe)
        .arg("--board-bake")
        .spawn()
        .map(|_| ())
        .map_err(|e| Error::new(E_FAIL, e.to_string()))
}
/// `--board-bake`: the whole bake, with the folder next to the settings, unless the board was
/// hidden meanwhile.
pub fn process() -> Result<()> {
    background(
        lock_name(),
        BACKGROUND_LOCK_WAIT_MS,
        || {
            Ok(Settings::load()
                .map_err(crate::settings::unreadable)?
                .board_visible)
        },
        || bake(&super::folder()?, &Wallpaper::new()?).map(|_| ()),
    )
}
/// A bake nobody waits for: it gives up quietly when the lock stays taken for `wait_ms`, and
/// runs only if the board is still `visible` once it holds the lock.
fn background(
    name: PCWSTR,
    wait_ms: u32,
    visible: impl FnOnce() -> Result<bool>,
    bake: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let Some(_lock) = Lock::wait(name, wait_ms)? else {
        return Ok(());
    };
    // ORDER: read under the lock. The Show board switch, turned off, saves the visibility before
    // `hide` takes the lock, so a bake that was waiting behind it never shows the board again.
    if !visible()? {
        return Ok(());
    }
    bake()
}

#[cfg(test)]
mod tests {
    use super::*;
    use shotoshot_core::{
        Object, Point, Rect, Shape, Style,
        board::{Gathered, Screen},
    };

    /// A 40 by 20 screen with its top left corner at (`x`, `y`).
    fn screen_at(id: &str, x: f64, y: f64) -> Screen {
        Screen {
            id: id.into(),
            bounds: Rect::new(
                Point { x, y },
                Point {
                    x: x + 40.,
                    y: y + 20.,
                },
            )
            .unwrap(),
        }
    }
    fn screen(id: &str, x: f64) -> Screen {
        screen_at(id, x, 0.)
    }
    fn bgra(image: &crate::Image, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * image.width + x) * 4) as usize;
        image.pixels[i..i + 4].try_into().unwrap()
    }

    #[test]
    fn a_relative_picture_path_is_made_absolute_and_keeps_its_file_name() {
        let relative = Path::new("target")
            .join("board-diagnostic-set")
            .join("baked-a.png");
        assert!(relative.is_relative());
        let path = absolute(&relative).unwrap();
        assert!(path.is_absolute(), "{path:?}");
        assert_eq!(path.file_name(), relative.file_name());
        assert!(path.ends_with(&relative), "{path:?}");
        assert!(!path.to_string_lossy().starts_with(r"\\?\"), "{path:?}");
    }

    #[test]
    fn the_frame_covers_the_union_of_the_screens_with_each_base() {
        let screens = [screen("a", -40.), screen("b", 0.)];
        let base = Base {
            position: Position::Fill,
            color: [1, 2, 3],
            screens: vec![],
        };
        let frame = frame(&base, &screens).unwrap();
        assert_eq!(
            (frame.left, frame.top, frame.width, frame.height),
            (-40, 0, 80, 20)
        );
        assert_eq!(frame.pixels.len(), 80 * 20 * 4);
        // Screen a's first pixel, screen b's first pixel, the last pixel: all the colour.
        for (x, y) in [(0, 0), (40, 0), (79, 19)] {
            assert_eq!(bgra(&frame, x, y), [3, 2, 1, 255], "({x}, {y})");
        }
    }

    #[test]
    fn each_screen_base_lands_at_its_place_when_the_union_starts_above_zero() {
        let _com = crate::ComApartment::new().unwrap();
        let dir = std::env::temp_dir().join(format!("shotoshot-bake-frame-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let red = dir.join("red.png");
        crate::save_png(&crate::Image::solid(2, 1, [0, 0, 255, 255]), &red).unwrap();
        // a is above the origin, b below and to its right: the union is (0, -20)..(80, 20) and
        // the corners (0, 0)..(40, 20) and (40, -20)..(80, 0) belong to no screen.
        let screens = [screen_at("a", 0., -20.), screen_at("b", 40., 0.)];
        let base = Base {
            position: Position::Fill,
            color: [1, 2, 3],
            screens: vec![base::ScreenBase {
                id: "b".into(),
                image: Some(red),
                baked: None,
            }],
        };
        let frame = frame(&base, &screens).unwrap();
        assert_eq!(
            (frame.left, frame.top, frame.width, frame.height),
            (0, -20, 80, 40)
        );
        assert_eq!(bgra(&frame, 10, 10), [3, 2, 1, 255], "a shows the colour");
        let inside_b = bgra(&frame, 50, 30);
        assert!(
            inside_b[2] >= 254 && inside_b[0] <= 1 && inside_b[1] <= 1,
            "b shows its red picture: {inside_b:?}"
        );
        assert_eq!(bgra(&frame, 10, 30), [0, 0, 0, 255], "below a: no screen");
        assert_eq!(bgra(&frame, 50, 10), [0, 0, 0, 255], "above b: no screen");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn hide_gives_back_only_pictures_that_still_exist_and_clears_for_the_rest() {
        let screen_base = |id: &str, image: Option<&str>| base::ScreenBase {
            id: id.into(),
            image: image.map(Into::into),
            baked: Some(format!("C:\\baked-{id}.png").into()),
        };
        let base = Base {
            position: Position::Fit,
            color: [1, 2, 3],
            screens: vec![
                screen_base("a", Some("C:\\a.jpg")),
                screen_base("b", Some("C:\\deleted.jpg")),
                screen_base("c", None),
                screen_base("gone", Some("C:\\a.jpg")),
            ],
        };
        let exists = |path: &Path| path == Path::new("C:\\a.jpg");
        let ids = |restore: &Restore| -> Vec<String> {
            restore
                .pictures
                .iter()
                .map(|(id, _)| id.to_string())
                .collect()
        };

        // Every present screen has a picture that exists: nothing to clear, and the unplugged
        // "gone" screen of the base is none of our business.
        let screens = [screen("a", 0.)];
        let restore = what_to_restore(&base, &screens, exists);
        assert_eq!((ids(&restore), restore.clear), (vec!["a".into()], false));
        assert_eq!(restore.pictures[0].1, Path::new("C:\\a.jpg"));

        // A picture deleted since, a plain colour and a screen the base never knew all mean
        // "no picture": the desktop is cleared first and only `a` gets its picture back.
        for lacking in ["b", "c", "new"] {
            let screens = [screen("a", 0.), screen(lacking, 40.)];
            let restore = what_to_restore(&base, &screens, exists);
            assert_eq!(
                (ids(&restore), restore.clear),
                (vec!["a".into()], true),
                "{lacking}"
            );
        }

        // No present screen has a usable picture.
        let screens = [screen("b", 0.)];
        let restore = what_to_restore(&base, &screens, exists);
        assert_eq!((ids(&restore), restore.clear), (vec![], true));
    }

    fn picture(path: &str) -> Option<PathBuf> {
        Some(PathBuf::from(path))
    }

    #[test]
    fn a_desktop_that_shows_the_base_has_nothing_unsettled() {
        let expected = [("a", Some(Path::new("C:\\a.jpg"))), ("b", None)];
        // The shell may spell the picture another way.
        let shown = [picture("c:/A.JPG"), None];
        assert!(unsettled(&expected, &shown).is_empty());
    }

    #[test]
    fn a_picture_left_on_a_clear_screen_asks_for_the_clear_and_every_base_picture_again() {
        let (a, c) = (Path::new("C:\\a.jpg"), Path::new("C:\\c.jpg"));
        let expected = [("a", Some(a)), ("b", None), ("c", Some(c))];
        // a and c are right, but the clear that b needs wipes them as well.
        let shown = [
            picture("C:\\a.jpg"),
            picture("C:\\baked-b.png"),
            picture("C:\\c.jpg"),
        ];
        let wrong = unsettled(&expected, &shown);
        assert!(wrong.clear);
        assert_eq!(wrong.pictures, vec![("a", a), ("c", c)]);
    }

    #[test]
    fn without_a_clear_only_the_pictures_that_are_not_right_are_set_again() {
        let (a, b) = (Path::new("C:\\a.jpg"), Path::new("C:\\b.jpg"));
        let expected = [("a", Some(a)), ("b", Some(b)), ("c", None)];
        // a still shows ours, b is right, c is clear as it should be.
        let shown = [picture("C:\\baked-a.png"), picture("C:\\b.jpg"), None];
        let wrong = unsettled(&expected, &shown);
        assert_eq!((wrong.clear, wrong.pictures), (false, vec![("a", a)]));
        // A restored screen that shows nothing is not right either.
        let shown = [None, picture("C:\\b.jpg"), None];
        assert_eq!(unsettled(&expected, &shown).pictures, vec![("a", a)]);
        // A picture of the base's on the wrong screen is none of its own.
        let shown = [picture("C:\\b.jpg"), picture("C:\\a.jpg"), None];
        assert_eq!(
            unsettled(&expected, &shown).pictures,
            vec![("a", a), ("b", b)]
        );
    }

    #[test]
    fn settle_issues_what_is_missing_until_the_desktop_shows_the_base() {
        use std::cell::{Cell, RefCell};
        let a = Path::new("C:\\a.jpg");
        let expected = [("a", Some(a)), ("b", None)];
        let (looks, fixes) = (Cell::new(0), RefCell::new(Vec::new()));
        // The pending sets win for two looks, then the shell has applied the base.
        let result = settle(
            &expected,
            Ok(()),
            || {
                looks.set(looks.get() + 1);
                Ok(if looks.get() < 3 {
                    vec![picture("C:\\baked-a.png"), picture("C:\\baked-b.png")]
                } else {
                    vec![picture("C:\\a.jpg"), None]
                })
            },
            |wrong| {
                fixes
                    .borrow_mut()
                    .push((wrong.clear, wrong.pictures.clone()));
                Ok(())
            },
            Duration::from_millis(1),
            Duration::from_secs(60),
        );
        assert!(result.is_ok());
        assert_eq!(looks.get(), 3);
        assert_eq!(*fixes.borrow(), vec![(true, vec![("a", a)]); 2]);
    }

    #[test]
    fn settle_forgets_an_error_that_a_retry_repaired() {
        let expected = [("a", None)];
        let result = settle(
            &expected,
            Err(Error::new(E_FAIL, "The clear failed")),
            || Ok(vec![None]),
            |_| panic!("nothing to fix"),
            Duration::from_millis(1),
            Duration::from_secs(60),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn settle_gives_up_after_the_timeout_with_the_first_error_or_its_own() {
        let a = Path::new("C:\\a.jpg");
        let expected = [("a", Some(a))];
        let (poll, timeout) = (Duration::from_millis(1), Duration::from_millis(30));
        let never = || Ok(vec![picture("C:\\baked-a.png")]);

        let own = settle(&expected, Ok(()), never, |_| Ok(()), poll, timeout).unwrap_err();
        assert_eq!(own.message(), "The desktop did not take the base back");

        let earlier = Err(Error::new(E_FAIL, "The position failed"));
        let fix_failing = |_: &Unsettled| Err(Error::new(E_FAIL, "The set failed"));
        let error = settle(&expected, earlier, never, fix_failing, poll, timeout).unwrap_err();
        assert_eq!(error.message(), "The position failed");

        let error = settle(&expected, Ok(()), never, fix_failing, poll, timeout).unwrap_err();
        assert_eq!(error.message(), "The set failed");

        let unreadable = || Err(Error::new(E_FAIL, "The desktop cannot be read"));
        let error = settle(&expected, Ok(()), unreadable, |_| Ok(()), poll, timeout).unwrap_err();
        assert_eq!(error.message(), "The desktop cannot be read");
    }

    /// A base of two screens, each with a picture of the user's and a baked one of ours.
    fn base_of_two_screens() -> Base {
        let screen_base = |id: &str| base::ScreenBase {
            id: id.into(),
            image: Some(format!("C:\\{id}.jpg").into()),
            baked: Some(format!("C:\\board\\baked-{id}-0123456789abcdef.png").into()),
        };
        Base {
            position: Position::Fit,
            color: [1, 2, 3],
            screens: vec![screen_base("a"), screen_base("b")],
        }
    }
    fn shown_on(a: Option<PathBuf>, b: Option<PathBuf>) -> Vec<(String, Option<PathBuf>)> {
        vec![("a".into(), a), ("b".into(), b)]
    }

    #[test]
    fn a_desktop_that_no_longer_shows_the_board_is_left_to_its_user() {
        // Hidden since (the base is back), another wallpaper chosen since, or a plain colour.
        for shown in [
            shown_on(picture("C:\\a.jpg"), picture("C:\\b.jpg")),
            shown_on(picture("C:\\chosen-since.jpg"), picture("C:\\b.jpg")),
            shown_on(None, None),
        ] {
            let kept = base_to_give_back(base_of_two_screens(), &shown, Position::Fill, [9, 9, 9]);
            assert_eq!(kept, None, "{shown:?}");
        }
    }

    #[test]
    fn a_screen_that_shows_the_board_gets_its_base_back() {
        let shown = shown_on(
            picture("C:\\board\\baked-a-0123456789abcdef.png"),
            picture("C:\\board\\baked-b-0123456789abcdef.png"),
        );
        // Every screen shows ours: the desktop's Fill is our own, the base keeps its position.
        let base = base_to_give_back(base_of_two_screens(), &shown, Position::Fill, [9, 9, 9]);
        assert_eq!(base, Some(base_of_two_screens()));
    }

    #[test]
    fn a_picture_chosen_since_the_last_bake_is_given_back_not_replaced() {
        // `b` got another wallpaper while Shotoshot was not running; `a` still shows the board.
        let shown = shown_on(
            picture("C:\\board\\baked-a-0123456789abcdef.png"),
            picture("C:\\chosen-since.jpg"),
        );
        let base = base_to_give_back(base_of_two_screens(), &shown, Position::Stretch, [9, 9, 9])
            .expect("a shows the board");
        let screens = [screen("a", 0.), screen("b", 40.)];
        let restore = what_to_restore(&base, &screens, |_| true);
        assert_eq!(
            restore.pictures,
            vec![
                ("a", Path::new("C:\\a.jpg")),
                ("b", Path::new("C:\\chosen-since.jpg"))
            ]
        );
        assert!(!restore.clear);
        assert_eq!(
            (base.position, base.color),
            (Position::Stretch, [9, 9, 9]),
            "a screen that is the user's makes the desktop's position and colour theirs"
        );
    }

    #[test]
    fn a_picture_named_like_ours_in_another_folder_still_counts_as_the_board() {
        // An earlier installation's bake: not the path the base recorded, yet never the user's.
        let shown = shown_on(
            picture("D:\\old\\board\\baked-a-fedcba9876543210.png"),
            picture("C:\\b.jpg"),
        );
        let base = base_to_give_back(base_of_two_screens(), &shown, Position::Fit, [1, 2, 3]);
        assert_eq!(base, Some(base_of_two_screens()));
    }

    #[test]
    fn a_note_across_two_screens_is_cut_consistently() {
        let _com = crate::ComApartment::new().unwrap();
        let screens = [screen("a", -40.), screen("b", 0.)];
        let base = Base {
            position: Position::Fill,
            color: [255, 255, 255],
            screens: vec![],
        };
        let frame = frame(&base, &screens).unwrap();
        let gathered = Gathered {
            objects: vec![Object {
                id: 1,
                rotation: 0.,
                shape: Shape::Rectangle(
                    Rect::new(Point { x: -10., y: 5. }, Point { x: 10., y: 15. }).unwrap(),
                ),
                style: Style {
                    color: [255, 0, 0],
                    width: 2.,
                },
                fill: vec![shotoshot_core::Paint::solid([255, 0, 0])],
            }],
            last_id: 1,
            background: Vec::new(),
        };
        let renderer =
            crate::render::Renderer::with(crate::render::Backend::Software, false).unwrap();
        let pictures = render_screens(&renderer, &frame, &gathered, &screens).unwrap();
        assert_eq!(pictures.len(), 2);
        let (a, b) = (&pictures[0].1, &pictures[1].1);
        assert_eq!((a.width, a.height), (40, 20));
        let px = |img: &crate::Image, x: u32, y: u32| {
            let i = ((y * img.width + x) * 4) as usize;
            [img.pixels[i + 2], img.pixels[i + 1], img.pixels[i]]
        };
        assert_eq!(
            px(a, 35, 10),
            [255, 0, 0],
            "right edge of screen a is inside the note"
        );
        assert_eq!(
            px(b, 5, 10),
            [255, 0, 0],
            "left edge of screen b continues it"
        );
        assert_eq!(px(a, 2, 2), [255, 255, 255]);
    }

    #[test]
    fn a_second_bake_gives_up_while_the_first_holds_the_lock() {
        use windows::core::{HSTRING, PCWSTR};
        // Its own name: never the real one, which a bake of the user's may hold right now.
        let name = HSTRING::from(format!(
            "Local\\Shotoshot.BoardBakeTest{}",
            std::process::id()
        ));
        let held = Lock::acquire_as(PCWSTR(name.as_ptr()), 1000).unwrap();
        // A mutex is owned per thread: the contender must be another thread.
        let contend = |wait_ms: u32| {
            let name = name.clone();
            std::thread::spawn(move || {
                Lock::acquire_as(PCWSTR(name.as_ptr()), wait_ms).map(|_lock| ())
            })
            .join()
            .unwrap()
        };
        let refused = contend(50).unwrap_err();
        assert_eq!(refused.message(), "Another bake is still running");
        drop(held);
        contend(1000).expect("free again once the first one is done");
    }

    #[test]
    fn a_background_bake_leaves_quietly_when_busy_and_bakes_only_a_visible_board() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use windows::core::{HSTRING, PCWSTR};
        let name = HSTRING::from(format!(
            "Local\\Shotoshot.BoardBakeBackgroundTest{}",
            std::process::id()
        ));
        let (asked, baked) = (AtomicU32::new(0), AtomicU32::new(0));
        // Another thread, as another process would be: a mutex is owned per thread.
        let attempt = |visible: bool| {
            std::thread::scope(|scope| {
                scope
                    .spawn(|| {
                        background(
                            PCWSTR(name.as_ptr()),
                            50,
                            || {
                                asked.fetch_add(1, Ordering::SeqCst);
                                Ok(visible)
                            },
                            || {
                                baked.fetch_add(1, Ordering::SeqCst);
                                Ok(())
                            },
                        )
                    })
                    .join()
                    .unwrap()
            })
        };
        let held = Lock::acquire_as(PCWSTR(name.as_ptr()), 1000).unwrap();
        attempt(true).expect("a busy lock is no error for a background bake");
        assert_eq!(
            (asked.load(Ordering::SeqCst), baked.load(Ordering::SeqCst)),
            (0, 0),
            "it left without reading the visibility or baking"
        );
        drop(held);
        attempt(false).unwrap();
        assert_eq!(
            (asked.load(Ordering::SeqCst), baked.load(Ordering::SeqCst)),
            (1, 0),
            "a board hidden meanwhile is not baked"
        );
        attempt(true).unwrap();
        assert_eq!(baked.load(Ordering::SeqCst), 1, "a visible board is");
    }
}
