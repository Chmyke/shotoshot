//! ROLE: The Board document: desktop notes anchored to their screen, one scene per screen.
//! INVARIANTS: `decode` gives back what `encode` wrote and accepts nothing it does not fully
//! understand; a screen that is absent today keeps its notes untouched.
use crate::{Attachment, Object, Paint, Point, Rect, Shape, remap_walls, renumber, shoto::Scene};
use std::fmt::Write;

pub const HEADER: &str = "shotoshot-board";
/// The latest version read; a board is written in the lowest one that holds its scenes,
/// the version of the scene text they share (`Scene::version`).
pub const VERSION: u32 = 2;
const MAX_SCREENS: usize = 64;

/// Why a board file does not load.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Written by a later version of the format.
    Newer,
    /// Truncated, altered or inconsistent.
    Damaged,
}
impl Error {
    /// The text the user reads.
    pub fn key(self) -> shotoshot_text::Key {
        match self {
            Self::Newer => shotoshot_text::Key::BoardNewer,
            Self::Damaged => shotoshot_text::Key::BoardDamaged,
        }
    }
}

/// The notes of one screen, in that screen's coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct ScreenNotes {
    /// Stable device path of the screen, a single token without whitespace.
    pub id: String,
    pub scene: Scene,
}

/// A screen present today.
#[derive(Clone, Debug, PartialEq)]
pub struct Screen {
    pub id: String,
    /// Desktop coordinates.
    pub bounds: Rect,
}

/// Every present screen's notes, in desktop coordinates, as one editor document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Gathered {
    pub objects: Vec<Object>,
    pub last_id: u64,
    pub background: Vec<Paint>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Board {
    pub screens: Vec<ScreenNotes>,
}

fn token(id: &str) -> bool {
    !id.is_empty() && id.len() <= 512 && id.bytes().all(|b| b > b' ' && b != 0x7f)
}

impl Board {
    pub fn encode(&self) -> String {
        let version = (self.screens.iter())
            .map(|n| n.scene.version())
            .fold(1, u32::max);
        let mut out = format!("{HEADER} {version}\n");
        for notes in self.screens.iter().filter(|n| token(&n.id)) {
            let text = notes.scene.to_text();
            // Writing to a String cannot fail.
            let _ = writeln!(out, "screen {} {}", notes.id, text.len());
            out.push_str(&text);
        }
        out
    }
    pub fn decode(text: &str) -> Result<Self, Error> {
        let (header, mut rest) = text.split_once('\n').ok_or(Error::Damaged)?;
        let version = header
            .strip_prefix(HEADER)
            .and_then(|v| v.strip_prefix(' '))
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|v| *v != 0)
            .ok_or(Error::Damaged)?;
        if version > VERSION {
            return Err(Error::Newer);
        }
        let mut screens = Vec::new();
        while !rest.is_empty() {
            let (line, after) = rest.split_once('\n').ok_or(Error::Damaged)?;
            let mut fields = line.split(' ');
            let (Some("screen"), Some(id), Some(len), None) =
                (fields.next(), fields.next(), fields.next(), fields.next())
            else {
                return Err(Error::Damaged);
            };
            let len: usize = len.parse().map_err(|_| Error::Damaged)?;
            if !token(id) || screens.len() >= MAX_SCREENS {
                return Err(Error::Damaged);
            }
            let (scene, next) = after.split_at_checked(len).ok_or(Error::Damaged)?;
            let scene = Scene::from_text(scene, version).ok_or(Error::Damaged)?;
            if screens.iter().any(|s: &ScreenNotes| s.id == id) {
                return Err(Error::Damaged);
            }
            screens.push(ScreenNotes {
                id: id.into(),
                scene,
            });
            rest = next;
        }
        Ok(Self { screens })
    }

    /// The notes of every present screen, moved to desktop coordinates and renumbered so ids
    /// never collide between screens. Attachments to objects left out are dropped.
    pub fn gather(&self, screens: &[Screen]) -> Gathered {
        let mut gathered = Gathered::default();
        let mut next = 1;
        for screen in screens {
            let Some(notes) = self.screens.iter().find(|n| n.id == screen.id) else {
                continue;
            };
            let first = next;
            let map = |id: u64| -> Option<u64> {
                let index = notes.scene.objects.iter().position(|o| o.id == id)?;
                Some(first + index as u64)
            };
            for (index, object) in notes.scene.objects.iter().enumerate() {
                let mut object = object.clone();
                object.id = first + index as u64;
                remap_walls(&mut object.fill, map);
                if let Shape::Arrow {
                    start_attach,
                    end_attach,
                    ..
                } = &mut object.shape
                {
                    for attach in [start_attach, end_attach] {
                        *attach = attach.and_then(|a| {
                            Some(Attachment {
                                target: map(a.target)?,
                                ..a
                            })
                        });
                    }
                }
                object.translate(screen.bounds.min);
                gathered.objects.push(object);
                next = first + index as u64 + 1;
            }
            let mut background = notes.scene.background.clone();
            remap_walls(&mut background, map);
            // `scatter` writes the same coats to every screen, each keeping its own walls.
            if gathered.background.is_empty() {
                gathered.background = background;
            } else if same_colours(&gathered.background, &background) {
                for (coat, more) in gathered.background.iter_mut().zip(background) {
                    coat.walls.extend(more.walls);
                }
            }
        }
        gathered.last_id = next - 1;
        gathered
    }

    /// Stores `gathered` (desktop coordinates) into the present screens: each object goes to the
    /// screen holding the centre of its bounds, or the nearest screen. Absent screens keep
    /// their notes. A screen that had no scene gets one only if it holds something, so empty
    /// scenes never pile up toward `MAX_SCREENS`; one that had a scene keeps it, emptied.
    pub fn scatter(&mut self, gathered: &Gathered, screens: &[Screen]) {
        let all = &gathered.objects;
        let anchor = |object: &Object| -> Option<usize> {
            let centre = object.resolved_bounds(all).center();
            screens
                .iter()
                .position(|s| s.bounds.contains(centre))
                .or_else(|| {
                    screens
                        .iter()
                        .enumerate()
                        .map(|(i, s)| (i, distance(s.bounds, centre)))
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                        .map(|(i, _)| i)
                })
        };
        let anchors: Vec<Option<usize>> = all.iter().map(anchor).collect();
        for (index, screen) in screens.iter().enumerate() {
            let held = || {
                all.iter()
                    .zip(&anchors)
                    .filter(|(_, anchor)| **anchor == Some(index))
                    .map(|(object, _)| object)
            };
            let ids: Vec<u64> = held().map(|o| o.id).collect();
            let mut objects: Vec<Object> = held()
                .map(|o| {
                    let mut o = o.clone();
                    let bound = |a: Option<Attachment>| a.is_some_and(|a| !ids.contains(&a.target));
                    if let Shape::Arrow {
                        start_attach,
                        end_attach,
                        ..
                    } = o.shape
                        && (bound(start_attach) || bound(end_attach))
                    {
                        // An end bound to an object on another screen stays where it is, unbound.
                        o.prepare_transform(all, &ids);
                    }
                    o.translate(Point::default() - screen.bounds.min);
                    o
                })
                .collect();
            let mut background = gathered.background.clone();
            remap_walls(&mut background, |wall| ids.contains(&wall).then_some(wall));
            // The pile's order, not its ids: restacking and erasing leave them apart.
            renumber(&mut objects, &mut background, 1);
            let stored = self.screens.iter().position(|n| n.id == screen.id);
            if stored.is_none() && objects.is_empty() && background.is_empty() {
                continue;
            }
            // Never below an object's id, or the scene would not read back.
            let last_id = objects
                .iter()
                .map(|o| o.id)
                .fold(gathered.last_id, u64::max);
            let size = screen.bounds.size();
            let (width, height) = (size.x.max(1.) as u32, size.y.max(1.) as u32);
            let scene = Scene {
                width,
                height,
                origin: (screen.bounds.min.x as i32, screen.bounds.min.y as i32),
                region: Rect {
                    min: Point::default(),
                    max: Point {
                        x: f64::from(width),
                        y: f64::from(height),
                    },
                },
                background,
                objects,
                last_id,
            };
            match stored {
                Some(index) => self.screens[index].scene = scene,
                None => self.screens.push(ScreenNotes {
                    id: screen.id.clone(),
                    scene,
                }),
            }
        }
    }
}

fn same_colours(a: &[Paint], b: &[Paint]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| a.color == b.color)
}

/// Distance from `p` to the nearest point of `r`; zero inside.
fn distance(r: Rect, p: Point) -> f64 {
    let dx = (r.min.x - p.x).max(p.x - r.max.x).max(0.);
    let dy = (r.min.y - p.y).max(p.y - r.max.y).max(0.);
    dx.hypot(dy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Style;

    #[test]
    fn refusals_are_keys() {
        assert_eq!(Error::Newer.key(), shotoshot_text::Key::BoardNewer);
        assert_eq!(Error::Damaged.key(), shotoshot_text::Key::BoardDamaged);
    }

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Rect {
        Rect {
            min: Point { x: x0, y: y0 },
            max: Point { x: x1, y: y1 },
        }
    }
    fn screen(id: &str, x: f64, y: f64) -> Screen {
        Screen {
            id: id.into(),
            bounds: rect(x, y, x + 1920., y + 1080.),
        }
    }
    fn rectangle(id: u64, x: f64, y: f64) -> Object {
        Object {
            id,
            rotation: 0.,
            shape: Shape::Rectangle(rect(x, y, x + 100., y + 50.)),
            style: Style {
                color: [10, 20, 30],
                width: 4.,
            },
            fill: Vec::new(),
        }
    }
    fn arrow(id: u64, start: Point, end: Point, end_attach: Option<Attachment>) -> Object {
        Object {
            id,
            rotation: 0.,
            shape: Shape::Arrow {
                start,
                end,
                start_attach: None,
                end_attach,
                head: true,
            },
            style: Style {
                color: [0, 0, 0],
                width: 3.,
            },
            fill: Vec::new(),
        }
    }
    fn desktop(objects: Vec<Object>, last_id: u64) -> Gathered {
        Gathered {
            objects,
            last_id,
            background: Vec::new(),
        }
    }

    #[test]
    fn an_empty_board_encodes_to_its_header_and_decodes() {
        let text = Board::default().encode();
        assert_eq!(text, "shotoshot-board 1\n");
        assert_eq!(Board::decode(&text), Ok(Board::default()));
    }

    #[test]
    fn scatter_then_encode_then_decode_then_gather_roundtrips() {
        let screens = [screen("left", -1920., 0.), screen("main", 0., 0.)];
        let gathered = desktop(
            vec![rectangle(1, -1000., 100.), rectangle(2, 300., 300.)],
            2,
        );
        let mut board = Board::default();
        board.scatter(&gathered, &screens);
        assert_eq!(board.screens.len(), 2);
        assert_eq!(
            board.screens[0].scene.objects[0].bounds().min,
            Point { x: 920., y: 100. }
        );
        let decoded = Board::decode(&board.encode()).unwrap();
        assert_eq!(decoded, board);
        let again = decoded.gather(&screens);
        assert_eq!(again.objects.len(), 2);
        assert_eq!(again.objects[0].bounds().min, Point { x: -1000., y: 100. });
        assert_eq!(again.objects[1].bounds().min, Point { x: 300., y: 300. });
        assert!(again.objects[0].id < again.objects[1].id);
        assert_eq!(again.last_id, again.objects[1].id);
    }

    #[test]
    fn spared_shapes_survive_a_round_trip_across_screens() {
        let screens = [screen("left", -1920., 0.), screen("main", 0., 0.)];
        let (dark, light) = ([1, 2, 3], [200, 210, 220]);
        let mut framed = rectangle(2, 300., 300.);
        framed.fill = vec![Paint {
            color: light,
            walls: vec![3],
        }];
        let gathered = Gathered {
            background: vec![
                Paint::solid(dark),
                Paint {
                    color: light,
                    walls: vec![1, 3],
                },
            ],
            ..desktop(
                vec![rectangle(1, -1000., 100.), framed, rectangle(3, 320., 310.)],
                3,
            )
        };
        let mut board = Board::default();
        board.scatter(&gathered, &screens);
        let text = board.encode();
        assert!(text.starts_with("shotoshot-board 2\n"));
        let again = Board::decode(&text).unwrap().gather(&screens);
        let id = |n: usize| again.objects[n].id;
        assert_eq!(again.objects[1].fill[0].walls, [id(2)]);
        assert_eq!(again.background[0], Paint::solid(dark));
        let mut walls = again.background[1].walls.clone();
        walls.sort();
        assert_eq!(walls, [id(0), id(2)], "each screen gave back its own walls");
    }

    #[test]
    fn objects_are_anchored_by_their_centre() {
        let screens = [screen("main", 0., 0.), screen("right", 1920., 0.)];
        // Centre at x = 1930: anchored to the right screen although it starts on the main one.
        let astride = rectangle(1, 1880., 0.);
        let outside = rectangle(2, 5000., 5000.);
        let mut board = Board::default();
        board.scatter(&desktop(vec![astride, outside], 2), &screens);
        let right = board.screens.iter().find(|s| s.id == "right").unwrap();
        assert_eq!(
            right.scene.objects.len(),
            2,
            "astride by centre, outside by nearest screen"
        );
        assert_eq!(
            right.scene.objects[0].bounds().min,
            Point { x: -40., y: 0. }
        );
        assert!(
            board.screens.iter().all(|s| s.id != "main"),
            "no scene for a screen that holds nothing"
        );
    }

    #[test]
    fn empty_scenes_are_not_stored_but_a_cleared_screen_stays_cleared() {
        let screens = [screen("a", 0., 0.), screen("b", 1920., 0.)];
        let mut board = Board::default();
        board.scatter(&desktop(vec![rectangle(1, 10., 10.)], 1), &screens);
        let ids: Vec<&str> = board.screens.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["a"], "b holds nothing");
        // Everything erased: a keeps an empty scene, so the next read shows it empty.
        board.scatter(&desktop(vec![], 1), &screens);
        assert_eq!(board.screens.len(), 1);
        assert!(board.screens[0].scene.objects.is_empty());
        assert!(board.gather(&screens).objects.is_empty());
        // A background alone is something to keep, on every present screen.
        board.scatter(
            &Gathered {
                background: vec![Paint::solid([1, 2, 3])],
                ..desktop(vec![], 1)
            },
            &screens,
        );
        assert_eq!(board.screens.len(), 2);
        // Screens plugged in one after another never fill the file with empty scenes.
        let mut board = Board::default();
        for i in 0..2 * MAX_SCREENS {
            board.scatter(&desktop(vec![], 0), &[screen(&format!("s{i}"), 0., 0.)]);
        }
        assert!(board.screens.is_empty());
        assert!(Board::decode(&board.encode()).is_ok());
    }

    #[test]
    fn a_stored_scene_never_has_an_id_above_its_last_id() {
        let screens = [screen("a", 0., 0.)];
        let mut board = Board::default();
        // An editor that lost count: the object's id is above the document's last id.
        board.scatter(&desktop(vec![rectangle(9, 10., 10.)], 3), &screens);
        let scene = &board.screens[0].scene;
        assert!(scene.objects.iter().all(|o| o.id <= scene.last_id));
        assert_eq!(Board::decode(&board.encode()), Ok(board.clone()));
        // A higher last id is kept as it is.
        board.scatter(&desktop(vec![rectangle(9, 10., 10.)], 12), &screens);
        assert_eq!(board.screens[0].scene.last_id, 12);
    }

    #[test]
    fn absent_screens_keep_their_notes() {
        let three = [
            screen("a", -1920., 0.),
            screen("b", 0., 0.),
            screen("c", 1920., 0.),
        ];
        let mut board = Board::default();
        board.scatter(
            &desktop(
                vec![
                    rectangle(1, -1000., 0.),
                    rectangle(2, 10., 0.),
                    rectangle(3, 2000., 0.),
                ],
                3,
            ),
            &three,
        );
        let two = [three[0].clone(), three[1].clone()];
        let gathered_two = board.gather(&two);
        assert_eq!(gathered_two.objects.len(), 2);
        // Editing with two screens must not touch the third screen's notes.
        board.scatter(&desktop(vec![], 0), &two);
        let c = board.screens.iter().find(|s| s.id == "c").unwrap();
        assert_eq!(c.scene.objects.len(), 1);
        assert_eq!(board.gather(&three).objects.len(), 1);
    }

    #[test]
    fn ids_are_unique_after_gathering_two_screens() {
        let screens = [screen("a", 0., 0.), screen("b", 1920., 0.)];
        let mut board = Board::default();
        board.scatter(&desktop(vec![rectangle(7, 0., 0.)], 7), &screens[..1]);
        board.scatter(&desktop(vec![rectangle(7, 1930., 0.)], 7), &screens[1..]);
        let gathered = board.gather(&screens);
        assert_eq!(gathered.objects.len(), 2);
        assert_ne!(gathered.objects[0].id, gathered.objects[1].id);
        assert_eq!(
            gathered.last_id,
            gathered.objects.iter().map(|o| o.id).max().unwrap()
        );
    }

    #[test]
    fn an_arrow_bound_to_a_shape_on_another_screen_is_unbound() {
        let screens = [screen("a", 0., 0.), screen("b", 1920., 0.)];
        // The target is on screen a. The arrow's resolved bounds reach back to it, but their
        // centre, x = 2601, is on screen b: the arrow goes there and must not keep a binding
        // to an object that is stored elsewhere.
        let target = rectangle(1, 100., 100.);
        let arrow = arrow(
            2,
            Point { x: 5000., y: 500. },
            Point { x: 2600., y: 300. },
            Some(Attachment {
                target: 1,
                direction: Point { x: 1., y: 0. },
            }),
        );
        let mut board = Board::default();
        board.scatter(&desktop(vec![target, arrow], 2), &screens);
        let b = board.screens.iter().find(|s| s.id == "b").unwrap();
        let Shape::Arrow {
            end, end_attach, ..
        } = b.scene.objects[0].shape
        else {
            panic!("arrow")
        };
        assert_eq!(end_attach, None);
        // Frozen on the target's right edge (x = 202, y = 125), not left at the stale
        // fallback point, then moved into screen b's coordinates.
        assert!((end.x - (202. - 1920.)).abs() < 1e-9 && (end.y - 125.).abs() < 1e-9);
        assert!(Board::decode(&board.encode()).is_ok());
    }

    #[test]
    fn a_restacked_pile_is_stored_in_its_order_with_its_bindings() {
        let screens = [screen("a", 0., 0.)];
        let mut board = Board::default();
        let bound = arrow(
            3,
            Point { x: 500., y: 30. },
            Point { x: 50., y: 30. },
            Some(Attachment {
                target: 7,
                direction: Point { x: 1., y: 0. },
            }),
        );
        // The box was raised above the arrow drawn after it.
        board.scatter(&desktop(vec![bound, rectangle(7, 0., 0.)], 7), &screens);
        assert_eq!(Board::decode(&board.encode()), Ok(board.clone()));
        let objects = &board.screens[0].scene.objects;
        assert!(matches!(objects[0].shape, Shape::Arrow { .. }));
        assert!(matches!(objects[1].shape, Shape::Rectangle(_)));
        let Shape::Arrow { end_attach, .. } = objects[0].shape else {
            panic!("arrow")
        };
        assert_eq!(end_attach.map(|a| a.target), Some(objects[1].id));
    }

    #[test]
    fn gathering_remaps_attachments_to_the_renumbered_targets() {
        let screens = [screen("a", 0., 0.), screen("b", 1920., 0.)];
        let mut board = Board::default();
        board.scatter(&desktop(vec![rectangle(7, 0., 0.)], 8), &screens[..1]);
        let bound = arrow(
            8,
            Point { x: 2500., y: 300. },
            Point { x: 2100., y: 300. },
            Some(Attachment {
                target: 7,
                direction: Point { x: 1., y: 0. },
            }),
        );
        board.scatter(
            &desktop(vec![rectangle(7, 1930., 0.), bound], 8),
            &screens[1..],
        );
        let gathered = board.gather(&screens);
        let ids: Vec<u64> = gathered.objects.iter().map(|o| o.id).collect();
        assert_eq!(ids, [1, 2, 3]);
        let Shape::Arrow { end_attach, .. } = gathered.objects[2].shape else {
            panic!("arrow")
        };
        assert_eq!(end_attach.map(|a| a.target), Some(2));
    }

    #[test]
    fn newer_or_damaged_files_are_refused() {
        assert_eq!(Board::decode("shotoshot-board 3\n"), Err(Error::Newer));
        assert_eq!(Board::decode("shotoshot-board 0\n"), Err(Error::Damaged));
        assert_eq!(Board::decode("shotoshot-board 1"), Err(Error::Damaged));
        assert_eq!(
            Board::decode("shotoshot-board 1\nscreen a 3\nxyz"),
            Err(Error::Damaged)
        );
        assert_eq!(
            Board::decode("shotoshot-board 1\nscreen a b 1\nx\n"),
            Err(Error::Damaged)
        );
        assert_eq!(Board::decode(""), Err(Error::Damaged));
        let mut board = Board::default();
        board.scatter(
            &Gathered {
                background: vec![Paint::solid([1, 2, 3])],
                ..desktop(vec![rectangle(1, 1., 1.)], 1)
            },
            &[screen("a", 0., 0.)],
        );
        let text = board.encode();
        for cut in [text.len() - 1, text.len() / 2, 20] {
            assert_eq!(
                Board::decode(&text[..cut]),
                Err(Error::Damaged),
                "cut at {cut}"
            );
        }
    }
}
