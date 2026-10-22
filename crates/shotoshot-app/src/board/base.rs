//! ROLE: The user's own background per screen (picture or colour), kept so the board can be
//! drawn over it and the desktop given back when the board is hidden.
//! INVARIANTS: One of our baked pictures, or any picture named like one, is never taken as a
//! base; the file is read strictly.
use super::wallpaper::{Position, Wallpaper};
use crate::Image;
// Kept here for the board's callers; the copy itself belongs with the image code.
pub(crate) use crate::image::blit;
use shotoshot_core::{Point, Rect, board::Screen};
use std::path::{Path, PathBuf};
use windows::{
    Win32::Foundation::E_FAIL,
    core::{Error, Result},
};

const HEADER: &str = "shotoshot-board-base";
const VERSION: u32 = 1;

/// What one screen showed before the board, and the picture we last baked for it.
#[derive(Clone, Debug, PartialEq)]
pub struct ScreenBase {
    pub id: String,
    /// The user's picture; `None` for a solid colour.
    pub image: Option<PathBuf>,
    /// Our own baked picture, set on the desktop while the board is shown.
    pub baked: Option<PathBuf>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Base {
    pub position: Position,
    pub color: [u8; 3],
    pub screens: Vec<ScreenBase>,
}

/// Windows paths ignore case and take either separator: the shell may hand back a picture of
/// ours spelled differently from the way we set it, and that must not make it look foreign.
pub(super) fn same_picture(a: &Option<PathBuf>, b: &Option<PathBuf>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => same_file(a, b),
        _ => false,
    }
}
fn same_file(a: &Path, b: &Path) -> bool {
    let key = |p: &Path| p.to_string_lossy().replace('/', "\\").to_lowercase();
    if key(a) == key(b) {
        return true;
    }
    // Spelled another way (a short name, a `\\?\` prefix, a `.` step): the same name, ignoring
    // case, in a folder that is the same one on disk.
    let name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().to_lowercase());
    let folder = |p: &Path| p.parent().and_then(|d| std::fs::canonicalize(d).ok());
    name(a).is_some() && name(a) == name(b) && folder(a).is_some() && folder(a) == folder(b)
}

/// Whether `path` is named like one of our baked pictures, `baked-<letters and digits>-<16 hex
/// digits>.png` (`board::slug`), whatever its folder: an earlier install's or a diagnostic's is
/// ours too.
pub fn is_baked_name(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let name = name.to_ascii_lowercase();
    let Some(stem) = name
        .strip_prefix("baked-")
        .and_then(|n| n.strip_suffix(".png"))
    else {
        return false;
    };
    stem.rsplit_once('-')
        .is_some_and(|(_, hash)| hash.len() == 16 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
}
/// What a screen showing `picture` may take as its user's own: a picture named like ours
/// counts as no picture.
fn usable(picture: &Option<PathBuf>) -> Option<PathBuf> {
    picture.clone().filter(|p| !is_baked_name(p))
}
/// The picture each of `screens` displays now.
pub(super) fn shown(
    wallpaper: &Wallpaper,
    screens: &[Screen],
) -> Result<Vec<(String, Option<PathBuf>)>> {
    screens
        .iter()
        .map(|screen| Ok((screen.id.clone(), wallpaper.shown(&screen.id)?)))
        .collect()
}

impl Base {
    /// One line per value, paths written as they are: a Windows path never holds a newline.
    pub fn encode(&self) -> String {
        let mut out = format!(
            "{HEADER} {VERSION}\nposition {}\ncolor {:02x}{:02x}{:02x}\n",
            self.position.index(),
            self.color[0],
            self.color[1],
            self.color[2]
        );
        let path = |p: &Option<PathBuf>| {
            p.as_deref()
                .map_or(String::new(), |p| p.display().to_string())
        };
        for s in &self.screens {
            out.push_str(&format!(
                "screen {}\nimage {}\nbaked {}\n",
                s.id,
                path(&s.image),
                path(&s.baked)
            ));
        }
        out
    }
    /// Strict: an unknown version, a missing line or a bad colour gives `None`.
    pub fn decode(text: &str) -> Option<Self> {
        let mut lines = text.strip_suffix('\n')?.split('\n');
        let version: u32 = lines
            .next()?
            .strip_prefix(HEADER)?
            .strip_prefix(' ')?
            .parse()
            .ok()?;
        if version != VERSION {
            return None;
        }
        let position =
            Position::from_index(lines.next()?.strip_prefix("position ")?.parse().ok()?)?;
        let color = lines.next()?.strip_prefix("color ")?;
        if color.len() != 6 || !color.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let value = u32::from_str_radix(color, 16).ok()?;
        let [_, r, g, b] = value.to_be_bytes();
        let mut screens = Vec::new();
        let mut lines = lines.peekable();
        while lines.peek().is_some() {
            let id = lines.next()?.strip_prefix("screen ")?;
            let image = lines.next()?.strip_prefix("image ")?;
            let baked = lines.next()?.strip_prefix("baked ")?;
            if id.is_empty() || id.contains(char::is_whitespace) {
                return None;
            }
            let path = |t: &str| (!t.is_empty()).then(|| PathBuf::from(t));
            screens.push(ScreenBase {
                id: id.into(),
                image: path(image),
                baked: path(baked),
            });
        }
        Some(Self {
            position,
            color: [r, g, b],
            screens,
        })
    }
    /// `None` when there is no file yet; a damaged file is an error and is left untouched.
    pub fn read(folder: &Path) -> Result<Option<Self>> {
        match std::fs::read_to_string(super::base_path(folder)) {
            Ok(text) => Self::decode(&text).map(Some).ok_or_else(|| {
                crate::failure::visible(shotoshot_text::Key::ErrorBoardBaseDamaged, &[])
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Error::new(E_FAIL, e.to_string())),
        }
    }
    pub fn write(&self, folder: &Path) -> Result<()> {
        crate::save_bytes(self.encode().as_bytes(), &super::base_path(folder), false).map(|_| ())
    }
    /// What the desktop shows right now, taken as the base: first use.
    pub fn capture(wallpaper: &Wallpaper, screens: &[Screen]) -> Result<Self> {
        Ok(Self::captured(
            &shown(wallpaper, screens)?,
            wallpaper.position()?,
            wallpaper.color()?,
        ))
    }
    /// The pure part of `capture`: `shown` is what each screen displays now. A screen showing a
    /// picture named like ours gets no picture (its colour).
    pub fn captured(
        shown: &[(String, Option<PathBuf>)],
        position: Position,
        color: [u8; 3],
    ) -> Self {
        Self {
            position,
            color,
            screens: shown
                .iter()
                .map(|(id, picture)| ScreenBase {
                    id: id.clone(),
                    image: usable(picture),
                    baked: None,
                })
                .collect(),
        }
    }
    /// Takes the desktop as the base wherever it is not ours: any picture that is not ours becomes
    /// the base of its screen (new screens are added), and while a screen shows something that is
    /// not ours, even no picture at all, the desktop's position and colour are the user's too. A
    /// picture named like ours is neither adopted nor foreign. True when something changed.
    pub fn adopt(&mut self, wallpaper: &Wallpaper, screens: &[Screen]) -> Result<bool> {
        Ok(self.adopt_desktop(
            &shown(wallpaper, screens)?,
            wallpaper.position()?,
            wallpaper.color()?,
        ))
    }
    /// The pure part of `adopt`: `shown` is what each screen displays now, `position` and `color`
    /// what the desktop uses now. While every screen shows one of our baked pictures (or one named
    /// like ours), position and colour are ours and are left alone.
    pub fn adopt_desktop(
        &mut self,
        shown: &[(String, Option<PathBuf>)],
        position: Position,
        color: [u8; 3],
    ) -> bool {
        // PITFALL: decided before the pictures are adopted; it reads `baked`, which adoption
        // never touches, and a screen unknown until now has nothing of ours.
        let foreign = shown.iter().any(|(id, picture)| {
            !self.shows_ours(id, picture) && !picture.as_deref().is_some_and(is_baked_name)
        });
        let mut changed = self.adopt_shown(shown);
        if foreign && (self.position, self.color) != (position, color) {
            self.position = position;
            self.color = color;
            changed = true;
        }
        changed
    }
    /// Whether screen `id` displays `picture` and that is its baked picture.
    pub(super) fn shows_ours(&self, id: &str, picture: &Option<PathBuf>) -> bool {
        self.screen(id)
            .is_some_and(|s| s.baked.is_some() && same_picture(&s.baked, picture))
    }
    /// The pictures part of `adopt`: `shown` is what each screen displays now. A known screen
    /// showing a picture named like ours keeps its base; a new one starts with no picture.
    pub fn adopt_shown(&mut self, shown: &[(String, Option<PathBuf>)]) -> bool {
        let mut changed = false;
        for (id, picture) in shown {
            match self.screens.iter_mut().find(|s| s.id == *id) {
                Some(s) => {
                    let ours = (s.baked.is_some() && same_picture(&s.baked, picture))
                        || picture.as_deref().is_some_and(is_baked_name);
                    if !ours && !same_picture(&s.image, picture) {
                        s.image = picture.clone();
                        changed = true;
                    }
                }
                None => {
                    self.screens.push(ScreenBase {
                        id: id.clone(),
                        image: usable(picture),
                        baked: None,
                    });
                    changed = true;
                }
            }
        }
        changed
    }
    /// Whether every present screen shows one of our baked pictures.
    pub fn all_ours(&self, wallpaper: &Wallpaper, screens: &[Screen]) -> Result<bool> {
        for screen in screens {
            if !self.shows_ours(&screen.id, &wallpaper.shown(&screen.id)?) {
                return Ok(false);
            }
        }
        Ok(true)
    }
    pub fn set_baked(&mut self, id: &str, path: PathBuf) {
        match self.screens.iter_mut().find(|s| s.id == id) {
            Some(s) => s.baked = Some(path),
            None => self.screens.push(ScreenBase {
                id: id.into(),
                image: None,
                baked: Some(path),
            }),
        }
    }
    pub fn screen(&self, id: &str) -> Option<&ScreenBase> {
        self.screens.iter().find(|s| s.id == id)
    }
}

/// Where a `source` picture lands on a `screen`, in screen coordinates, for Windows' modes.
/// Tile and Span are drawn like Fill: documented approximation of this first version.
pub fn placement(position: Position, source: (u32, u32), screen: (u32, u32)) -> Rect {
    let (sw, sh) = (source.0.max(1) as f64, source.1.max(1) as f64);
    let (w, h) = (screen.0 as f64, screen.1 as f64);
    let centred = |dw: f64, dh: f64| Rect {
        min: Point {
            x: (w - dw) / 2.,
            y: (h - dh) / 2.,
        },
        max: Point {
            x: (w + dw) / 2.,
            y: (h + dh) / 2.,
        },
    };
    match position {
        Position::Stretch => Rect {
            min: Point::default(),
            max: Point { x: w, y: h },
        },
        Position::Center => centred(sw, sh),
        Position::Fit => {
            let s = (w / sw).min(h / sh);
            centred(sw * s, sh * s)
        }
        Position::Fill | Position::Tile | Position::Span => {
            let s = (w / sw).max(h / sh);
            centred(sw * s, sh * s)
        }
    }
}

/// The base of one screen as a picture of exactly `size`.
pub fn render(
    base: &ScreenBase,
    position: Position,
    color: [u8; 3],
    size: (u32, u32),
) -> Result<Image> {
    let mut image = Image::solid(size.0, size.1, [color[2], color[1], color[0], 255]);
    let Some(path) = &base.image else {
        return Ok(image);
    };
    // A picture that is gone or unreadable shows the colour, as the desktop itself does.
    let Ok(source) = crate::load_png(path) else {
        return Ok(image);
    };
    let dst = placement(position, (source.width, source.height), size);
    let scaled = crate::export::scale(
        &source,
        dst.size().x.round().max(1.) as u32,
        dst.size().y.round().max(1.) as u32,
    )?;
    blit(
        &mut image,
        &scaled,
        dst.min.x.round() as i32,
        dst.min.y.round() as i32,
    );
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_follows_windows_modes() {
        let fill = placement(Position::Fill, (100, 50), (200, 200));
        assert_eq!(
            (fill.min.x, fill.min.y, fill.max.x, fill.max.y),
            (-100., 0., 300., 200.)
        );
        let fit = placement(Position::Fit, (100, 50), (200, 200));
        assert_eq!(
            (fit.min.x, fit.min.y, fit.max.x, fit.max.y),
            (0., 50., 200., 150.)
        );
        let stretch = placement(Position::Stretch, (100, 50), (200, 200));
        assert_eq!(
            (stretch.min.x, stretch.max.x, stretch.max.y),
            (0., 200., 200.)
        );
        let center = placement(Position::Center, (100, 50), (200, 200));
        assert_eq!((center.min.x, center.min.y), (50., 75.));
        assert_eq!(placement(Position::Span, (100, 50), (200, 200)), fill);
        assert_eq!(placement(Position::Tile, (100, 50), (200, 200)), fill);
    }

    fn two_screens() -> Base {
        Base {
            position: Position::Fit,
            color: [81, 92, 107],
            screens: vec![
                ScreenBase {
                    id: "\\\\?\\DISPLAY#A".into(),
                    image: Some("D:\\pictures\\mé moi\\img 0.jpg".into()),
                    baked: None,
                },
                ScreenBase {
                    id: "\\\\?\\DISPLAY#B".into(),
                    image: None,
                    baked: Some("C:\\x\\baked-B.png".into()),
                },
            ],
        }
    }

    #[test]
    fn base_text_roundtrips_with_paths_holding_spaces() {
        let base = two_screens();
        assert_eq!(Base::decode(&base.encode()), Some(base.clone()));
        assert_eq!(Base::decode("shotoshot-board-base 2\n"), None);
        assert_eq!(Base::decode(&base.encode()[..30]), None);
    }

    #[test]
    fn a_damaged_base_file_is_refused_not_guessed() {
        let text = two_screens().encode();
        assert!(Base::decode(&text).is_some());
        // A colour that is not six hexadecimal digits.
        assert_eq!(
            Base::decode(&text.replace("color 515c6b", "color 515c6")),
            None
        );
        assert_eq!(
            Base::decode(&text.replace("color 515c6b", "color +15c6b")),
            None
        );
        // An unknown position, a screen cut after its id, an id that is not a single token.
        assert_eq!(
            Base::decode(&text.replace("position 3", "position 9")),
            None
        );
        assert_eq!(Base::decode(&text.replace("baked \n", "")), None);
        assert_eq!(Base::decode(&text.replace("DISPLAY#A", "DISPLAY A")), None);
        // No screen at all is a valid, empty base.
        let empty = Base {
            screens: Vec::new(),
            ..two_screens()
        };
        assert_eq!(Base::decode(&empty.encode()), Some(empty));
    }

    #[test]
    fn read_and_write_go_through_the_base_file() {
        let folder =
            std::env::temp_dir().join(format!("shotoshot-base-file-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        assert_eq!(Base::read(&folder).unwrap(), None, "no file yet");
        let base = two_screens();
        base.write(&folder).unwrap();
        assert_eq!(Base::read(&folder).unwrap(), Some(base));
        let file = crate::board::base_path(&folder);
        std::fs::write(&file, "damaged").unwrap();
        assert!(Base::read(&folder).is_err(), "a damaged file is an error");
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "damaged",
            "and is left untouched"
        );
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_solid_base_renders_the_colour_and_a_picture_is_scaled() {
        let _com = crate::ComApartment::new().unwrap();
        let solid = ScreenBase {
            id: "a".into(),
            image: None,
            baked: None,
        };
        let image = render(&solid, Position::Fill, [81, 92, 107], (4, 2)).unwrap();
        assert_eq!(
            &image.pixels[..4],
            &[107, 92, 81, 255],
            "BGRA of the colour"
        );
        let dir = std::env::temp_dir().join(format!("shotoshot-base-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("two.png");
        let mut two = crate::Image::solid(2, 1, [0, 0, 0, 255]);
        two.pixels[..4].copy_from_slice(&[0, 0, 255, 255]); // red on the left, black on the right
        crate::save_png(&two, &path).unwrap();
        let picture = ScreenBase {
            id: "b".into(),
            image: Some(path),
            baked: None,
        };
        let stretched = render(&picture, Position::Stretch, [0, 0, 0], (4, 2)).unwrap();
        assert_eq!((stretched.width, stretched.height), (4, 2));
        // A cubic scaler blends at the edges: the tolerance is the filter's, not the code's.
        assert!(stretched.pixels[2] >= 215, "left column red");
        assert!(stretched.pixels[4 * 3 + 2] <= 40, "right column black");
        let red: Vec<u8> = stretched.pixels[..16].chunks(4).map(|p| p[2]).collect();
        assert!(
            red.windows(2).all(|w| w[0] >= w[1]),
            "red fades left to right: {red:?}"
        );
        assert_eq!(
            stretched.pixels[16..],
            stretched.pixels[..16],
            "both rows alike"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn fit_leaves_bars_of_the_colour_and_fill_covers_the_screen() {
        let _com = crate::ComApartment::new().unwrap();
        let dir = std::env::temp_dir().join(format!("shotoshot-base-fit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("red.png");
        crate::save_png(&crate::Image::solid(2, 1, [0, 0, 255, 255]), &path).unwrap();
        let picture = ScreenBase {
            id: "a".into(),
            image: Some(path),
            baked: None,
        };
        let first_pixels = |image: &Image| -> Vec<Vec<u8>> {
            image
                .pixels
                .chunks(4 * image.width as usize)
                .map(|row| row[..4].to_vec())
                .collect()
        };
        let bar = vec![30, 20, 10, 255];
        let fit = render(&picture, Position::Fit, [10, 20, 30], (4, 4)).unwrap();
        let rows = first_pixels(&fit);
        assert_eq!(
            (rows[0].clone(), rows[3].clone()),
            (bar.clone(), bar.clone())
        );
        assert!(
            rows[1][2] >= 254 && rows[1][0] == 0,
            "red inside: {:?}",
            rows[1]
        );
        assert!(
            rows[2][2] >= 254 && rows[2][0] == 0,
            "red inside: {:?}",
            rows[2]
        );
        let fill = render(&picture, Position::Fill, [10, 20, 30], (4, 4)).unwrap();
        assert!(
            fill.pixels.chunks(4).all(|p| p[2] >= 254 && p[0] == 0),
            "no bar left"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_picture_that_cannot_be_read_shows_the_colour() {
        let _com = crate::ComApartment::new().unwrap();
        let gone = ScreenBase {
            id: "a".into(),
            image: Some("C:\\no\\such\\folder\\gone.jpg".into()),
            baked: None,
        };
        let image = render(&gone, Position::Fill, [1, 2, 3], (3, 2)).unwrap();
        assert_eq!((image.width, image.height), (3, 2));
        assert!(image.pixels.chunks(4).all(|p| p == [3, 2, 1, 255]));
    }

    #[test]
    fn adopt_takes_a_foreign_picture_as_base() {
        // Pure logic on a fake: the wallpaper now shows `C:\new.jpg` on screen "a" while our
        // baked file was `C:\baked-a.png`: the base must become `C:\new.jpg`.
        let mut base = Base {
            position: Position::Fill,
            color: [0; 3],
            screens: vec![ScreenBase {
                id: "a".into(),
                image: None,
                baked: Some("C:\\baked-a.png".into()),
            }],
        };
        let changed = base.adopt_shown(&[("a".to_string(), Some(PathBuf::from("C:\\new.jpg")))]);
        assert!(changed);
        assert_eq!(
            base.screens[0].image.as_deref(),
            Some(Path::new("C:\\new.jpg"))
        );
        let same = base.adopt_shown(&[("a".to_string(), Some(PathBuf::from("C:\\baked-a.png")))]);
        assert!(!same, "our own picture is never adopted");
        let shouting =
            base.adopt_shown(&[("a".to_string(), Some(PathBuf::from("c:/BAKED-A.png")))]);
        assert!(!shouting, "Windows paths ignore case and separators");
        let added = base.adopt_shown(&[("b".to_string(), None)]);
        assert!(added);
        assert_eq!(base.screens.len(), 2);
    }

    /// One screen "a" on a Fill desktop with the colour [1, 2, 3], optionally with a baked picture.
    fn one_screen_base(baked: Option<&str>) -> Base {
        Base {
            position: Position::Fill,
            color: [1, 2, 3],
            screens: vec![ScreenBase {
                id: "a".into(),
                image: None,
                baked: baked.map(PathBuf::from),
            }],
        }
    }
    fn shows(id: &str, picture: Option<&str>) -> (String, Option<PathBuf>) {
        (id.to_string(), picture.map(PathBuf::from))
    }

    #[test]
    fn a_colour_or_position_changed_on_a_desktop_without_picture_is_adopted() {
        let mut base = one_screen_base(None);
        let none = [shows("a", None)];
        assert!(base.adopt_desktop(&none, Position::Fill, [9, 9, 9]));
        assert_eq!((base.position, base.color), (Position::Fill, [9, 9, 9]));
        assert!(base.adopt_desktop(&none, Position::Center, [9, 9, 9]));
        assert_eq!((base.position, base.color), (Position::Center, [9, 9, 9]));
        assert!(
            !base.adopt_desktop(&none, Position::Center, [9, 9, 9]),
            "nothing differs: nothing changed"
        );
    }

    #[test]
    fn position_and_colour_stay_ours_while_every_screen_shows_our_picture() {
        let mut base = one_screen_base(Some("C:\\baked-a.png"));
        let ours = [shows("a", Some("C:\\baked-a.png"))];
        assert!(!base.adopt_desktop(&ours, Position::Center, [9, 9, 9]));
        assert_eq!((base.position, base.color), (Position::Fill, [1, 2, 3]));
    }

    #[test]
    fn leaving_our_picture_for_a_plain_colour_adopts_that_colour() {
        let mut base = one_screen_base(Some("C:\\baked-a.png"));
        assert!(base.adopt_desktop(&[shows("a", None)], Position::Fill, [9, 9, 9]));
        assert_eq!(base.color, [9, 9, 9]);
        assert_eq!(base.screens[0].image, None, "still a colour, no picture");
    }

    #[test]
    fn one_screen_that_is_not_ours_makes_the_desktop_the_base() {
        let mut base = Base {
            screens: vec![
                one_screen_base(Some("C:\\baked-a.png")).screens.remove(0),
                ScreenBase {
                    id: "b".into(),
                    image: None,
                    baked: Some("C:\\baked-b.png".into()),
                },
            ],
            ..one_screen_base(None)
        };
        let shown = [
            shows("a", Some("C:\\baked-a.png")),
            shows("b", Some("C:\\chosen.jpg")),
        ];
        assert!(base.adopt_desktop(&shown, Position::Fit, [9, 9, 9]));
        assert_eq!((base.position, base.color), (Position::Fit, [9, 9, 9]));
        assert_eq!(
            base.screen("b").unwrap().image,
            Some("C:\\chosen.jpg".into())
        );
        assert_eq!(base.screen("a").unwrap().image, None, "ours is not adopted");
        // A screen plugged in since the last bake has nothing of ours either.
        let mut base = one_screen_base(Some("C:\\baked-a.png"));
        let shown = [shows("a", Some("C:\\baked-a.png")), shows("c", None)];
        assert!(base.adopt_desktop(&shown, Position::Fill, [9, 9, 9]));
        assert_eq!((base.screens.len(), base.color), (2, [9, 9, 9]));
    }

    /// The file name a bake gives the picture of screen `id`.
    fn baked_name(id: &str) -> String {
        format!("baked-{}.png", crate::board::slug(id))
    }

    #[test]
    fn only_names_like_our_baked_pictures_count_as_baked() {
        let ours = baked_name("\\\\?\\DISPLAY#A");
        assert!(is_baked_name(Path::new(&ours)));
        assert!(
            is_baked_name(&Path::new("D:\\elsewhere").join(ours.to_uppercase())),
            "any folder, any case"
        );
        assert!(
            is_baked_name(Path::new("baked--0123456789abcdef.png")),
            "a screen id without letters or digits"
        );
        for other in [
            "baked-a.png",
            "baked-0123456789abcdef.png",
            "baked-A-0123456789abcdeg.png",
            "baked-A-0123456789abcde.png",
            "baked-A-0123456789abcdef.jpg",
            "my-baked-A-0123456789abcdef.png",
            "holiday.png",
        ] {
            assert!(!is_baked_name(Path::new(other)), "{other}");
        }
    }

    #[test]
    fn a_picture_named_like_ours_is_never_captured_as_a_base() {
        let stray = format!("D:\\old install\\{}", baked_name("a"));
        let shown = [shows("a", Some(&stray)), shows("b", Some("C:\\beach.jpg"))];
        let base = Base::captured(&shown, Position::Fit, [1, 2, 3]);
        assert_eq!(
            base.screen("a").unwrap().image,
            None,
            "no picture: the colour"
        );
        assert_eq!(
            base.screen("b").unwrap().image,
            Some("C:\\beach.jpg".into())
        );
        assert_eq!((base.position, base.color), (Position::Fit, [1, 2, 3]));
    }

    #[test]
    fn a_picture_named_like_ours_is_neither_adopted_nor_foreign() {
        let stray = format!(
            "C:\\repo\\target\\board-diagnostic-set\\{}",
            baked_name("a")
        );
        let mut base = one_screen_base(Some("C:\\baked-a.png"));
        base.screens[0].image = Some("C:\\beach.jpg".into());
        // Screen "a" shows a picture of ours from another folder; a new screen "c" too.
        let shown = [shows("a", Some(&stray)), shows("c", Some(&stray))];
        assert!(
            base.adopt_desktop(&shown, Position::Center, [9, 9, 9]),
            "c is new"
        );
        assert_eq!(
            base.screen("a").unwrap().image,
            Some("C:\\beach.jpg".into()),
            "a keeps its base"
        );
        assert_eq!(base.screen("c").unwrap().image, None, "c has no picture");
        assert_eq!(
            (base.position, base.color),
            (Position::Fill, [1, 2, 3]),
            "not foreign: position and colour stay ours"
        );
        assert!(!base.adopt_desktop(&shown, Position::Center, [9, 9, 9]));
    }

    #[test]
    fn our_picture_spelled_another_way_is_still_ours() {
        let folder =
            std::env::temp_dir().join(format!("shotoshot-base-spelling-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let name = baked_name("a");
        let set = folder.join(&name);
        let base = one_screen_base(Some(set.to_str().unwrap()));
        // What the shell may hand back: a verbatim path, another case and separator, a `.` step.
        let spellings = [
            PathBuf::from(format!("\\\\?\\{}", set.display())),
            PathBuf::from(set.display().to_string().to_uppercase().replace('\\', "/")),
            folder.join(".").join(name.to_uppercase()),
        ];
        for spelled in spellings {
            assert!(
                base.shows_ours("a", &Some(spelled.clone())),
                "{}",
                spelled.display()
            );
        }
        let elsewhere = std::env::temp_dir().join(&name);
        assert!(
            !base.shows_ours("a", &Some(elsewhere)),
            "the same name in another folder"
        );
        std::fs::remove_dir_all(folder).unwrap();
    }

    /// Reads the real desktop and never writes to it; `check.ps1` runs it on its own.
    #[test]
    #[ignore = "real desktop: reads the screens and what they show, run alone by check.ps1"]
    fn real_desktop_adopting_what_was_just_captured_changes_nothing() {
        let _com = crate::ComApartment::new().unwrap();
        let wallpaper = Wallpaper::new().unwrap();
        let screens = wallpaper.screens().unwrap();
        let mut base = Base::capture(&wallpaper, &screens).unwrap();
        assert!(!base.adopt(&wallpaper, &screens).unwrap());
    }
}
