//! ROLE: The board as an editor session: the desktop picture is the frame, the notes are the
//! objects, the whole frame is the region.
//! INVARIANTS: Closing never discards: the owner writes the notes before the window hides, or
//! keeps the session open.
use super::*;
use shotoshot_core::{
    board::{Gathered, Screen},
    shoto::Scene,
};
use std::path::PathBuf;

/// What a board session knows beyond its capture: the screens it opened on and the folder its
/// document lives in.
#[derive(Clone, Debug, PartialEq)]
pub struct BoardSession {
    pub screens: Vec<Screen>,
    pub folder: PathBuf,
}

impl Session {
    /// A session on the desktop picture `frame`, which covers `screens`, holding `gathered`
    /// (frame coordinates). `icons` is what the desktop lays over the notes: drawn over them
    /// here too, so the session shows what the desktop will. The toolbar follows the pointer
    /// from screen to screen. The notes are written back to `folder`.
    pub fn board(
        frame: Image,
        icons: Option<Image>,
        gathered: Gathered,
        screens: Vec<Screen>,
        folder: PathBuf,
    ) -> Result<Self> {
        let damaged = || Error::new(E_FAIL, "The board could not be opened.");
        let scene = Scene {
            width: frame.width,
            height: frame.height,
            origin: (frame.left, frame.top),
            region: Rect {
                min: Point::default(),
                max: Point {
                    x: frame.width as f64,
                    y: frame.height as f64,
                },
            },
            background: gathered.background,
            objects: gathered.objects,
            last_id: gathered.last_id,
        };
        let mut renderer = Renderer::with(Backend::Hardware, false)?;
        renderer.set_image(&frame)?;
        renderer.set_overlay(icons)?;
        let monitors = hint::monitors_from(frame.left, frame.top);
        let measure = |text: &str, size: f64| renderer.measure_text(text, size as f32).ok();
        let mut capture = Capture::reopen(scene, frame, monitors, measure).ok_or_else(damaged)?;
        capture.board = true;
        // A board is drawn in zones: it opens with the rectangle, a capture with the arrow.
        capture.editor.tool = shotoshot_core::Tool::Rectangle;
        capture.enable_views();
        Ok(Self {
            capture,
            native: Native {
                renderer,
                text: None,
                dimensions: None,
                screen: crate::image::Screen::Desktop,
                live_surface: None,
                live_sample: None,
                back_buffer: None,
                acceleration: None,
                caret: Default::default(),
            },
            document: None,
            board: Some(BoardSession { screens, folder }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shotoshot_core::{
        Object, Point, Rect, Shape, Style,
        board::{Gathered, Screen},
    };
    use shotoshot_ui::Monitor;
    #[test]
    fn a_board_session_shows_the_notes_on_the_whole_frame_and_is_not_a_capture() {
        let _com = crate::ComApartment::new().unwrap();
        let mut frame = crate::Image::solid(300, 100, [20, 20, 20, 255]);
        (frame.left, frame.top) = (-100, 0);
        let gathered = Gathered {
            objects: vec![Object {
                id: 4,
                rotation: 0.,
                shape: Shape::Rectangle(
                    Rect::new(Point { x: 10., y: 10. }, Point { x: 50., y: 40. }).unwrap(),
                ),
                style: Style {
                    color: [1, 2, 3],
                    width: 2.,
                },
                fill: Vec::new(),
            }],
            last_id: 4,
            background: Vec::new(),
        };
        let screens = vec![Screen {
            id: "left".into(),
            bounds: Rect::new(Point { x: -100., y: 0. }, Point { x: 200., y: 100. }).unwrap(),
        }];
        let icons = Image::solid(300, 100, [0, 0, 0, 0]);
        let folder = std::env::temp_dir();
        let session = Session::board(
            frame,
            Some(icons),
            gathered,
            screens.clone(),
            folder.clone(),
        )
        .unwrap();
        assert_eq!(session.board, Some(BoardSession { screens, folder }));
        assert!(
            session.native.renderer.overlay().is_some(),
            "the icons lie over the notes"
        );
        assert!(
            session.capture.board,
            "the tools go to the screen under the pointer"
        );
        assert_eq!(session.document, None);
        assert!(!session.capture.selecting);
        assert_eq!(
            session.capture.region,
            Rect::new(Point::default(), Point { x: 300., y: 100. })
        );
        assert!(
            session.capture.reopened,
            "live mode is greyed like a reopened document"
        );
        assert_eq!(
            session.capture.editor.objects()[0].bounds().min,
            Point { x: 10., y: 10. }
        );
        assert_eq!(session.capture.editor.last_id(), 4);
    }
    #[test]
    fn the_icons_lie_over_the_notes_in_the_board_picture() {
        let _com = crate::ComApartment::new().unwrap();
        let (width, height) = (600, 400);
        let frame = Image::solid(width, height, [20, 20, 20, 255]);
        let mut icons = Image::solid(width, height, [0, 0, 0, 0]);
        // One opaque icon pixel on the note's stroke, away from the handles and the toolbar.
        let at = |x: usize, y: usize| (y * width as usize + x) * 4;
        icons.pixels[at(40, 50)..at(40, 50) + 4].copy_from_slice(&[10, 200, 30, 255]);
        let gathered = Gathered {
            objects: vec![Object {
                id: 1,
                rotation: 0.,
                shape: Shape::Rectangle(
                    Rect::new(Point { x: 40., y: 40. }, Point { x: 300., y: 200. }).unwrap(),
                ),
                style: Style {
                    color: [255, 0, 0],
                    width: 4.,
                },
                fill: Vec::new(),
            }],
            last_id: 1,
            background: Vec::new(),
        };
        let screens = vec![Screen {
            id: "one".into(),
            bounds: Rect::new(
                Point::default(),
                Point {
                    x: width as f64,
                    y: height as f64,
                },
            )
            .unwrap(),
        }];
        let mut session =
            Session::board(frame, Some(icons), gathered, screens, std::env::temp_dir()).unwrap();
        let exported = session.export().unwrap();
        let painted = crate::paint_image(width, height, |dc| session.paint(dc)).unwrap();
        for (picture, way) in [(&exported, "copied"), (&painted, "on screen")] {
            let pixel = |x: usize, y: usize| &picture.pixels[at(x, y)..at(x, y) + 3];
            assert_eq!(
                pixel(40, 50),
                [10, 200, 30],
                "{way}: the icon covers the note"
            );
            assert_eq!(
                pixel(40, 100),
                [0, 0, 255],
                "{way}: the note where nothing lies over it"
            );
            assert_eq!(
                pixel(300, 30),
                [20, 20, 20],
                "{way}: the frame through the transparency"
            );
        }
    }
    #[test]
    fn a_zoomed_screen_paints_its_notes_larger_from_its_own_corner_and_the_other_stays() {
        let _com = crate::ComApartment::new().unwrap();
        let (width, height) = (600usize, 200usize);
        let frame = Image::solid(width as u32, height as u32, [20, 20, 20, 255]);
        let note = |id: u64, x: f64| Object {
            id,
            rotation: 0.,
            shape: Shape::Rectangle(
                Rect::new(
                    Point { x, y: 40. },
                    Point {
                        x: x + 100.,
                        y: 140.,
                    },
                )
                .unwrap(),
            ),
            style: Style {
                color: [255, 255, 255],
                width: 4.,
            },
            fill: Vec::new(),
        };
        let gathered = Gathered {
            objects: vec![note(1, 40.), note(2, 340.)],
            last_id: 2,
            background: Vec::new(),
        };
        let left = Rect::new(Point::default(), Point { x: 300., y: 200. }).unwrap();
        let right = Rect::new(Point { x: 300., y: 0. }, Point { x: 600., y: 200. }).unwrap();
        let screens = vec![
            Screen {
                id: "left".into(),
                bounds: left,
            },
            Screen {
                id: "right".into(),
                bounds: right,
            },
        ];
        let mut session =
            Session::board(frame, None, gathered, screens, std::env::temp_dir()).unwrap();
        session.capture.toolbar_visible = false;
        // The real monitors of the machine make way for the two synthetic screens.
        session.capture.monitors = vec![Monitor::from(left), Monitor::from(right)];
        session.capture.enable_views();
        let exported = session.export().unwrap();
        let at = |image: &Image, x: usize, y: usize| image.pixels[(y * width + x) * 4];
        let paint = |session: &mut Session| {
            crate::paint_image(width as u32, height as u32, |dc| session.paint(dc)).unwrap()
        };
        let plain = paint(&mut session);
        assert_eq!(at(&plain, 40, 90), 255, "left edge of the left note at 1:1");
        // The left screen at 2x around its own corner: the edge at x = 40 lands at x = 80.
        session.capture.pointer = Point::default();
        session.capture.views[0].zoom = 2.;
        let zoomed = paint(&mut session);
        assert_eq!(
            at(&zoomed, 80, 180),
            255,
            "the edge moved to twice its distance"
        );
        assert_eq!(at(&zoomed, 40, 100), 20, "nothing left where it was");
        assert_eq!(at(&zoomed, 340, 90), 255, "the right screen is untouched");
        assert_eq!(
            at(&zoomed, 299, 100),
            20,
            "the left view stops at the edge of its screen"
        );
        assert_eq!(
            session.export().unwrap().pixels,
            exported.pixels,
            "the export ignores the view"
        );
    }
    #[test]
    fn a_board_session_may_zoom_and_a_capture_may_not() {
        let _com = crate::ComApartment::new().unwrap();
        let screens = vec![Screen {
            id: "one".into(),
            bounds: Rect::new(Point::default(), Point { x: 400., y: 200. }).unwrap(),
        }];
        let mut board = Session::board(
            Image::solid(400, 200, [0; 4]),
            None,
            Gathered::default(),
            screens,
            std::env::temp_dir(),
        )
        .unwrap();
        board.capture.pointer = Point { x: 10., y: 10. };
        assert!(board.zoom_wheel(120));
        assert!(board.capture.zoomed());
        let mut capture = Session::new(Image::solid(400, 200, [0; 4])).unwrap();
        capture.capture.pointer = Point { x: 10., y: 10. };
        assert!(!capture.zoom_wheel(120));
        assert!(!capture.capture.zoomed());
    }
    #[test]
    fn escape_in_a_board_keeps_the_note_being_typed_and_the_session() {
        let _com = crate::ComApartment::new().unwrap();
        // The text box is a native child window: it needs a parent, never shown.
        let parent = crate::resources::OwnedWindow(unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!(""),
                WS_POPUP,
                0,
                0,
                400,
                200,
                None,
                None,
                None,
                None,
            )
            .unwrap()
        });
        let screens = vec![Screen {
            id: "test".into(),
            bounds: Rect::new(Point::default(), Point { x: 400., y: 200. }).unwrap(),
        }];
        let frame = || Image::solid(400, 200, [0, 0, 0, 255]);
        let board = Session::board(
            frame(),
            None,
            Gathered::default(),
            screens,
            std::env::temp_dir(),
        )
        .unwrap();
        let capture = Session::new(frame()).unwrap();
        let mut kept = Vec::new();
        for mut session in [board, capture] {
            session
                .native
                .begin_text(&mut session.capture, parent.0, Point { x: 25., y: 35. })
                .unwrap();
            let input = session.native.text.as_ref().expect("inline text box");
            unsafe { SetWindowTextW(input.window, w!("Call Anna")).unwrap() };
            // Escape belongs to the text box: the session goes on.
            assert!(session.text_command(parent.0, 27, false));
            assert!(session.native.text.is_none());
            kept.push(
                session
                    .capture
                    .editor
                    .objects()
                    .iter()
                    .any(|o| matches!(&o.shape, Shape::Text { text, .. } if text == "Call Anna")),
            );
        }
        assert_eq!(
            kept,
            [true, false],
            "a board keeps the note, a capture drops it"
        );
    }
}
