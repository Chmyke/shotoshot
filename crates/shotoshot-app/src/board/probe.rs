//! ROLE: Proves the board path on the real machine: screens, base, rendering and files, without
//! touching the desktop; `run_set` is the explicit opt-in that really sets and restores it.
//! INVARIANTS: `run` never sets, clears, bakes or hides the wallpaper and writes only under
//! `target/`; only `run_set` changes the desktop, and it always tries to give the base back.
use super::{
    bake,
    base::{Base, same_picture},
    wallpaper::Wallpaper,
};
use crate::{
    Image,
    render::{Backend, Renderer},
};
use shotoshot_core::{
    Object, Point, Rect, Shape, Style,
    board::{Board, Gathered, Screen},
};
use std::path::{Path, PathBuf};
use windows::{
    Win32::Foundation::E_FAIL,
    core::{Error, Result},
};

const REPORT: &str = "target/board-diagnostics.txt";
/// The synthetic note: a rectangle, its corners this far from the screen's top left corner.
const NOTE_FROM: Point = Point { x: 40., y: 40. };
const NOTE_TO: Point = Point { x: 240., y: 140. };
const NOTE_COLOR: [u8; 3] = [237, 79, 92];
/// Where the middle of the rectangle's top edge lands in a screen's picture.
const NOTE_TOP_MIDDLE: (u32, u32) = (((NOTE_FROM.x + NOTE_TO.x) / 2.) as u32, NOTE_FROM.y as u32);

fn fail(text: &str) -> Error {
    Error::new(E_FAIL, text)
}
fn io(error: std::io::Error) -> Error {
    fail(&error.to_string())
}

/// One rectangle near the top left corner of every screen, stored the way a board is.
fn synthetic(screens: &[Screen]) -> Board {
    let objects = screens
        .iter()
        .enumerate()
        .map(|(i, screen)| Object {
            id: i as u64 + 1,
            rotation: 0.,
            shape: Shape::Rectangle(Rect {
                min: screen.bounds.min + NOTE_FROM,
                max: screen.bounds.min + NOTE_TO,
            }),
            style: Style {
                color: NOTE_COLOR,
                width: 6.,
            },
            fill: None,
        })
        .collect();
    let mut board = Board::default();
    let gathered = Gathered {
        objects,
        last_id: screens.len() as u64,
        background: None,
    };
    board.scatter(&gathered, screens);
    board
}

/// The screens the desktop reports; none is a failure, nothing could be proved without one.
fn present_screens(wallpaper: &Wallpaper) -> Result<Vec<Screen>> {
    let screens = wallpaper.screens()?;
    if screens.is_empty() {
        return Err(fail("No screen reported"));
    }
    Ok(screens)
}

/// A baked picture goes through the PNG file a bake would set: screen-sized, note on it.
fn check_round_trip(folder: &Path, screen: &Screen, picture: &Image) -> Result<()> {
    let path = folder.join(format!("baked-{}.png", super::slug(&screen.id)));
    crate::save_png(picture, &path)?;
    let read = crate::load_png(&path)?;
    let size = screen.bounds.size();
    if (read.width, read.height) != (size.x as u32, size.y as u32) {
        return Err(fail("A baked picture is not screen-sized"));
    }
    let at = ((NOTE_TOP_MIDDLE.1 * read.width + NOTE_TOP_MIDDLE.0) * 4) as usize;
    let pixel = read
        .pixels
        .get(at..at + 4)
        .ok_or_else(|| fail("A baked picture is too small for the note"))?;
    // BGRA rows; a stroke this wide covers the pixel whole, so only rounding may differ.
    let drawn = [pixel[2], pixel[1], pixel[0]];
    if drawn.iter().zip(NOTE_COLOR).any(|(a, b)| a.abs_diff(b) > 2) {
        return Err(fail("The note is not on the baked picture"));
    }
    Ok(())
}

/// Everything `run` proves, with its files under `root`; the report line when it all holds.
/// Reads the desktop (screens, what they show, Explorer's composition) and changes none of it.
fn inspect(root: &Path) -> Result<String> {
    let folder = root.join("board-diagnostic");
    std::fs::create_dir_all(&folder).map_err(io)?;
    let wallpaper = Wallpaper::new()?;
    let screens = present_screens(&wallpaper)?;
    let base = Base::capture(&wallpaper, &screens)?;
    std::fs::write(super::document_path(&folder), synthetic(&screens).encode()).map_err(io)?;
    // Read back as a bake reads it: through the file, then gathered for these screens.
    let gathered = bake::read_board(&folder)?.gather(&screens);
    if gathered.objects.len() != screens.len() {
        return Err(fail("The board file did not give back every note"));
    }
    let frame = bake::frame(&base, &screens)?;
    let renderer = Renderer::with(Backend::Software, false)?;
    let pictures = bake::render_screens(&renderer, &frame, &gathered, &screens)?;
    for (id, picture) in &pictures {
        let screen = screens
            .iter()
            .find(|s| s.id == *id)
            .ok_or_else(|| fail("A picture belongs to no screen"))?;
        check_round_trip(&folder, screen, picture)?;
    }
    let mut desktop = frame.clone();
    let icons = crate::image::render_desktop(&mut desktop).is_ok();
    Ok(format!(
        "PASS: {} screen(s), base {}, {} picture(s) baked in {}, desktop rendering {}.\n",
        screens.len(),
        if base.screens.iter().all(|s| s.image.is_none()) {
            "solid colour"
        } else {
            "pictures"
        },
        pictures.len(),
        folder.display(),
        if icons {
            "available"
        } else {
            "unavailable (base only)"
        },
    ))
}

/// `--board-diagnostic`: the whole path on this machine's screens, the desktop left as it is.
pub fn run() -> Result<()> {
    // A report left by an earlier run must never outlive a failure of this one.
    let _ = std::fs::remove_file(REPORT);
    let report = inspect(Path::new("target"))?;
    std::fs::write(REPORT, report).map_err(io)
}

/// Bakes the synthetic board and checks the desktop shows the pictures. The caller gives the
/// base back whatever this returns.
fn bake_and_look(folder: &Path, wallpaper: &Wallpaper, screens: &[Screen]) -> Result<()> {
    bake::run(folder, wallpaper)?;
    let base = Base::read(folder)?.ok_or_else(|| fail("No base written"))?;
    if !base.all_ours(wallpaper, screens)? {
        return Err(fail("The desktop does not show the baked pictures"));
    }
    Ok(())
}

/// What the desktop shows must be the base again: every picture, and the position.
fn check_given_back(folder: &Path, wallpaper: &Wallpaper, screens: &[Screen]) -> Result<()> {
    let base = Base::read(folder)?.ok_or_else(|| fail("No base"))?;
    for screen in screens {
        let expected = base.screen(&screen.id).and_then(|b| b.image.clone());
        if !same_picture(&wallpaper.shown(&screen.id)?, &expected) {
            return Err(fail("The base was not given back"));
        }
    }
    if wallpaper.position()? != base.position {
        return Err(fail("The base position was not given back"));
    }
    Ok(())
}

/// Whether the plain clear is worth one more try on a desktop that was not given back: the base
/// had no picture on any of these screens (so the clear cannot erase one it should have kept) and
/// one of them still shows the picture we baked. `shown` is what each of `screens` displays.
fn clear_is_last_resort(base: &Base, screens: &[Screen], shown: &[Option<PathBuf>]) -> bool {
    let kept = |screen: &Screen| base.screen(&screen.id).and_then(|b| b.image.as_ref());
    screens.iter().all(|s| kept(s).is_none())
        && screens
            .iter()
            .zip(shown)
            .any(|(s, picture)| base.shows_ours(&s.id, picture))
}
fn needs_last_resort(folder: &Path, wallpaper: &Wallpaper, screens: &[Screen]) -> Result<bool> {
    let Some(base) = Base::read(folder)? else {
        return Ok(false);
    };
    let shown = screens
        .iter()
        .map(|s| wallpaper.shown(&s.id))
        .collect::<Result<Vec<_>>>()?;
    Ok(clear_is_last_resort(&base, screens, &shown))
}

/// `check_given_back`, and when it fails on a desktop that still shows our pictures over a base
/// without any, the plain clear once more so the probe never leaves the desktop dirty. The verdict
/// stays a failure either way: the clear cleans up after `hide`, it does not excuse it. `hide`
/// now waits until the desktop shows the base, so this net should never be needed.
fn check_or_clean(folder: &Path, wallpaper: &Wallpaper, screens: &[Screen]) -> Result<()> {
    let Err(error) = check_given_back(folder, wallpaper, screens) else {
        return Ok(());
    };
    // A desktop that cannot even be read is not cleaned blindly: the check's own error stands.
    if !needs_last_resort(folder, wallpaper, screens).unwrap_or(false) {
        return Err(error);
    }
    let last_resort = match wallpaper.clear_all() {
        Err(e) => format!("the last-resort clear failed: {}", e.message()),
        Ok(()) => match check_given_back(folder, wallpaper, screens) {
            Ok(()) => "the last-resort clear then gave it back".to_string(),
            Err(e) => format!("the last-resort clear did not help: {}", e.message()),
        },
    };
    Err(fail(&format!("{}; {last_resort}", error.message())))
}

/// `--board-diagnostic-set`: really sets the baked pictures on every screen, checks what the
/// desktop reports, then gives the base back. Explicit opt-in: the desktop changes for a moment.
pub fn run_set() -> Result<()> {
    let _ = std::fs::remove_file(REPORT);
    // Absolute, as the board folder is in production: the wallpaper API reports absolute paths
    // and `Base` compares them with the ones recorded here.
    let folder = std::env::current_dir()
        .map_err(io)?
        .join("target")
        .join("board-diagnostic-set");
    std::fs::create_dir_all(&folder).map_err(io)?;
    // A base left by an earlier run is not what this desktop showed: with none, a bake that
    // fails before it changed anything leaves nothing to give back.
    match std::fs::remove_file(super::base_path(&folder)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(io(e)),
        _ => {}
    }
    let wallpaper = Wallpaper::new()?;
    let screens = present_screens(&wallpaper)?;
    std::fs::write(super::document_path(&folder), synthetic(&screens).encode()).map_err(io)?;
    // Every step is tried and every failure kept, each named by the step that raised it.
    let mut failures = Vec::new();
    let mut note = |step: &str, result: Result<()>| {
        if let Err(error) = result {
            failures.push(format!("{step}: {}", error.message()));
        }
    };
    note("bake", bake_and_look(&folder, &wallpaper, &screens));
    // ORDER: always, before any verdict is returned.
    note("hide", bake::hide(&folder, &wallpaper));
    // No base: the bake failed before it changed anything, so there is nothing to give back.
    if !matches!(Base::read(&folder), Ok(None)) {
        note("check", check_or_clean(&folder, &wallpaper, &screens));
    }
    if !failures.is_empty() {
        return Err(fail(&failures.join("; ")));
    }
    std::fs::write(
        REPORT,
        "PASS: baked pictures set on every screen and the base given back.\n",
    )
    .map_err(io)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(id: &str, x: f64) -> Screen {
        Screen {
            id: id.into(),
            bounds: Rect {
                min: Point { x, y: 0. },
                max: Point {
                    x: x + 800.,
                    y: 600.,
                },
            },
        }
    }

    #[test]
    fn the_synthetic_board_puts_one_note_on_each_screen_wherever_it_is() {
        let screens = [screen("a", -800.), screen("b", 0.), screen("c", 1000.)];
        let text = synthetic(&screens).encode();
        let gathered = Board::decode(&text).unwrap().gather(&screens);
        assert_eq!(gathered.objects.len(), screens.len());
        for (object, screen) in gathered.objects.iter().zip(&screens) {
            let Shape::Rectangle(rect) = &object.shape else {
                panic!("{:?}", object.shape);
            };
            assert_eq!(rect.min, screen.bounds.min + NOTE_FROM, "{}", screen.id);
            assert_eq!(rect.max, screen.bounds.min + NOTE_TO, "{}", screen.id);
            assert_eq!(object.style.color, NOTE_COLOR);
        }
    }

    #[test]
    fn the_plain_clear_is_a_last_resort_only_over_our_pictures_on_a_base_without_any() {
        use crate::board::{base::ScreenBase, wallpaper::Position};
        let ours = PathBuf::from(r"C:\baked-a.png");
        let theirs = PathBuf::from(r"C:\a.jpg");
        let screen_base = |id: &str, image: Option<&PathBuf>| ScreenBase {
            id: id.into(),
            image: image.cloned(),
            baked: Some(PathBuf::from(format!(r"C:\baked-{id}.png"))),
        };
        let base_of = |screens: Vec<ScreenBase>| Base {
            position: Position::Fit,
            color: [1, 2, 3],
            screens,
        };
        let screens = [screen("a", 0.), screen("b", 800.)];
        let colours = base_of(vec![screen_base("a", None), screen_base("b", None)]);
        let still_ours = [Some(ours), Some(PathBuf::from(r"C:\baked-b.png"))];
        // Our pictures are still up and the base has no picture to lose: clear.
        assert!(clear_is_last_resort(&colours, &screens, &still_ours));
        // A single one of ours is enough.
        assert!(clear_is_last_resort(
            &colours,
            &screens,
            &[None, still_ours[1].clone()]
        ));
        // Nothing of ours is up (colour, or a picture that is not ours): nothing to clean.
        assert!(!clear_is_last_resort(&colours, &screens, &[None, None]));
        assert!(!clear_is_last_resort(
            &colours,
            &screens,
            &[Some(theirs.clone()), None]
        ));
        // A base picture on any present screen: a clear would erase it.
        let picture = base_of(vec![
            screen_base("a", Some(&theirs)),
            screen_base("b", None),
        ]);
        assert!(!clear_is_last_resort(&picture, &screens, &still_ours));
        // A screen the base never knew shows nothing of ours: it does not block the others.
        let unknown = [screen("a", 0.), screen("new", 800.)];
        assert!(clear_is_last_resort(&colours, &unknown, &still_ours));
    }

    /// Reads the screens, what they show and Explorer's composition; sets nothing. `check.ps1`
    /// runs it on its own.
    #[test]
    #[ignore = "real desktop: reads the screens and Explorer's composition, run alone by check.ps1"]
    fn real_desktop_board_diagnostic_leaves_the_desktop_as_it_was() {
        let _com = crate::ComApartment::new().unwrap();
        let wallpaper = Wallpaper::new().unwrap();
        let screens = wallpaper.screens().unwrap();
        let state = || {
            let shown: Vec<_> = screens
                .iter()
                .map(|s| wallpaper.shown(&s.id).unwrap())
                .collect();
            (
                shown,
                wallpaper.position().unwrap(),
                wallpaper.color().unwrap(),
            )
        };
        let before = state();
        let root =
            std::env::temp_dir().join(format!("shotoshot-board-probe-{}", std::process::id()));
        let report = inspect(&root).unwrap();
        assert!(report.starts_with("PASS"), "{report}");
        assert_eq!(state(), before, "the diagnostic changed the desktop");
        std::fs::remove_dir_all(root).unwrap();
    }
}
