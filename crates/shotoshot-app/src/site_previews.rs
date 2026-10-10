//! ROLE: Website previews: the real capture UI and presentation aids over supplied desktop pictures.
//! The pictures come from the site repository; coordinates below match its fictional desktop.
use crate::*;
use shotoshot_core::*;
use shotoshot_ui::toolbar::Action;
use std::path::{Path, PathBuf};
use windows::Win32::{
    Foundation::{HWND, WPARAM},
    Graphics::Gdi::{DeleteObject, HDC},
    UI::WindowsAndMessaging::{
        CreateWindowExW, DI_NORMAL, DrawIconEx, GetIconInfo, HICON, ICONINFO, LoadCursorW,
        SendMessageW, WM_CHAR, WS_EX_TOOLWINDOW, WS_POPUP,
    },
};
use windows::core::{Error, PCWSTR, Result, w};

const RED: [u8; 3] = shotoshot_ui::toolbar::COLORS[0];
const YELLOW: [u8; 3] = shotoshot_ui::toolbar::COLORS[1];
const BLUE: [u8; 3] = shotoshot_ui::toolbar::COLORS[3];
const WHITE: [u8; 3] = shotoshot_ui::toolbar::COLORS[4];
/// Right edge of the captured area: it leaves the tool bar its default place, on the right.
const AREA_RIGHT: f64 = 1040.;

fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}

fn draw(editor: &mut Editor, tool: Tool, color: [u8; 3], from: Point, to: Point, ctrl: bool) {
    editor.tool = tool;
    editor.style.color = color;
    editor.begin(
        from,
        Modifiers {
            ctrl,
            ..Modifiers::default()
        },
    );
    editor.update(to);
    editor.finish();
}

/// Annotated launch board: framed card, highlighted subtitle, circled chart, bound arrow.
fn annotations() -> Editor {
    let mut editor = Editor::default();
    editor.style.width = 4.;
    draw(
        &mut editor,
        Tool::Rectangle,
        RED,
        p(172., 232.),
        p(474., 454.),
        false,
    );
    editor.style.color = RED;
    editor.add_text(p(178., 462.), "Title too low?".into());
    draw(
        &mut editor,
        Tool::Highlight,
        YELLOW,
        p(178., 211.),
        p(418., 211.),
        false,
    );
    draw(
        &mut editor,
        Tool::Ellipse,
        BLUE,
        p(600., 343.),
        p(706., 343.),
        false,
    );
    draw(
        &mut editor,
        Tool::Arrow,
        RED,
        p(930., 214.),
        p(640., 300.),
        true,
    );
    editor.style.color = WHITE;
    editor.add_text(p(884., 150.), "Mobile +12%".into());
    editor.tool = Tool::Rectangle;
    editor
}

fn session(backdrop: &Image, editor: Editor) -> Result<session::Session> {
    let mut session = session::Session::new(backdrop.clone())?;
    session.capture.editor = editor;
    session.capture.region = Rect::new(p(100., 86.), p(AREA_RIGHT, 612.));
    session.capture.selecting = false;
    Ok(session)
}

fn note_id(editor: &Editor) -> Result<u64> {
    editor
        .objects()
        .iter()
        .find(|o| matches!(&o.shape, Shape::Text { text, .. } if text.starts_with("Title")))
        .map(|o| o.id)
        .ok_or_else(|| Error::new(windows::Win32::Foundation::E_FAIL, "missing note"))
}

/// `point` turned around `center` by `degrees` (screen coordinates, clockwise positive).
fn turned(point: Point, center: Point, degrees: f64) -> Point {
    let (sin, cos) = degrees.to_radians().sin_cos();
    let (dx, dy) = (point.x - center.x, point.y - center.y);
    p(
        center.x + dx * cos - dy * sin,
        center.y + dx * sin + dy * cos,
    )
}

fn fail(message: &str) -> Error {
    Error::new(windows::Win32::Foundation::E_FAIL, message)
}

fn draw_native_cursor(hdc: HDC, name: PCWSTR, pointer: Point) -> Result<()> {
    // PITFALL: GetIconInfo allocates two GDI bitmaps even for a shared system cursor.
    unsafe {
        let cursor = LoadCursorW(None, name)?;
        let icon = HICON(cursor.0);
        let mut info = ICONINFO::default();
        GetIconInfo(icon, &mut info)?;
        // PITFALL: a system cursor is shared by every process. While other capture windows use it,
        // drawing it fails now and then with no error code, and succeeds when asked again (the
        // cross, once in about forty films beside a parallel test run, 2026-10-09).
        let mut result = Ok(());
        for _ in 0..3 {
            result = DrawIconEx(
                hdc,
                pointer.x.round() as i32 - info.xHotspot as i32,
                pointer.y.round() as i32 - info.yHotspot as i32,
                icon,
                0,
                0,
                0,
                None,
                DI_NORMAL,
            );
            if result.is_ok() {
                break;
            }
        }
        if !info.hbmMask.is_invalid() {
            let _ = DeleteObject(info.hbmMask.into());
        }
        if !info.hbmColor.is_invalid() {
            let _ = DeleteObject(info.hbmColor.into());
        }
        result
    }
}

fn heart_points(center: Point, scale: f64, segments: usize) -> Vec<Point> {
    (0..=segments)
        .map(|index| {
            let angle = std::f64::consts::TAU * index as f64 / segments as f64;
            Point {
                x: center.x + 16. * angle.sin().powi(3) * scale,
                y: center.y
                    - (13. * angle.cos()
                        - 5. * (2. * angle).cos()
                        - 2. * (3. * angle).cos()
                        - (4. * angle).cos())
                        * scale,
            }
        })
        .collect()
}

/// Where the segment `from`→`to` crosses the polyline `outline`, as ascending fractions of it.
fn crossings(outline: &[Point], from: Point, to: Point) -> Vec<f64> {
    let cross = |a: Point, b: Point| a.x * b.y - a.y * b.x;
    let axis = to - from;
    let mut hits: Vec<f64> = outline
        .windows(2)
        .filter_map(|edge| {
            let side = edge[1] - edge[0];
            let turn = cross(axis, side);
            if turn.abs() < f64::EPSILON {
                return None;
            }
            let offset = edge[0] - from;
            let (along, across) = (cross(offset, side) / turn, cross(offset, axis) / turn);
            ((0. ..=1.).contains(&along) && (0. ..1.).contains(&across)).then_some(along)
        })
        .collect();
    hits.sort_by(f64::total_cmp);
    hits
}

/// Frame-by-frame recording of real editor gestures. The site adds the cursor and encodes it,
/// unless `native_cursor` already painted the one the application chose.
struct Film {
    session: session::Session,
    folder: PathBuf,
    pointers: Vec<Point>,
    native_cursor: bool,
    /// Parent of the native text field; the editor-only film leaves it null.
    window: HWND,
}
impl Film {
    fn shoot(&mut self, pointer: Point) -> Result<()> {
        self.session.capture.pointer = pointer;
        let capture = &self.session.capture.image;
        let (width, height) = (capture.width, capture.height);
        let cursor = self.native_cursor.then(|| self.session.cursor(pointer));
        let frame = paint_image(width, height, |hdc| {
            self.session.paint(hdc)?;
            if let Some(name) = cursor {
                draw_native_cursor(hdc, name, pointer)?;
            }
            Ok(())
        })?;
        save_png(
            &frame,
            &self.folder.join(format!("{:03}.png", self.pointers.len())),
        )?;
        self.pointers.push(pointer);
        Ok(())
    }
    fn pointer(&self) -> Point {
        *self.pointers.last().unwrap_or(&p(0., 0.))
    }
    fn hold(&mut self, frames: usize) -> Result<()> {
        (0..frames).try_for_each(|_| self.shoot(self.pointer()))
    }
    /// Eased pointer travel; `step` applies each position to the editor (e.g. a drag).
    fn travel(
        &mut self,
        to: Point,
        frames: usize,
        mut step: impl FnMut(&mut Editor, Point),
    ) -> Result<()> {
        let from = self.pointer();
        for i in 1..=frames {
            let t = i as f64 / frames as f64;
            let t = t * t * (3. - 2. * t);
            let at = p(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t);
            step(&mut self.session.capture.editor, at);
            self.shoot(at)?;
        }
        Ok(())
    }
    fn press(&mut self) {
        let at = self.pointer();
        self.session.capture.editor.begin(at, Modifiers::default());
    }
    fn release(&mut self) {
        self.session.capture.editor.finish();
    }
    /// One pointer move through the capture's own state machine, as the window forwards it.
    fn step(&mut self, at: Point, modifiers: Modifiers) -> Result<()> {
        let (capture, mut host) = self.session.host(self.window);
        capture.pointer_move(&mut host, at, modifiers);
        self.shoot(at)
    }
    fn glide(&mut self, to: Point, frames: usize, modifiers: Modifiers) -> Result<()> {
        let from = self.pointer();
        for i in 1..=frames {
            let t = i as f64 / frames as f64;
            let t = t * t * (3. - 2. * t);
            self.step(
                p(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t),
                modifiers,
            )?;
        }
        Ok(())
    }
    /// Press as the window does: a toolbar hit runs its action, anything else starts a gesture.
    fn down(&mut self, modifiers: Modifiers) -> Result<()> {
        let at = self.pointer();
        let (capture, mut host) = self.session.host(self.window);
        if let Some(action) = capture.pointer_down(&mut host, at, modifiers)? {
            capture.action(&mut host, action);
        }
        Ok(())
    }
    fn up(&mut self, modifiers: Modifiers) {
        let at = self.pointer();
        let (capture, mut host) = self.session.host(self.window);
        capture.pointer_up(&mut host, at, modifiers);
    }
    fn click(&mut self) -> Result<()> {
        self.down(Modifiers::default())?;
        self.up(Modifiers::default());
        Ok(())
    }
    /// Travel to a toolbar button and click it.
    fn pick(&mut self, action: Action) -> Result<()> {
        let button = self
            .session
            .capture
            .buttons()
            .iter()
            .find(|b| b.action == action)
            .map(|b| b.bounds.center())
            .ok_or_else(|| fail("missing toolbar button"))?;
        self.glide(button, 10, Modifiers::default())?;
        self.click()?;
        self.hold(4)
    }
    /// Types into the open native text field, one character every `frames`.
    fn type_text(&mut self, text: &str, frames: usize) -> Result<()> {
        let field = self
            .session
            .native
            .text
            .as_ref()
            .map(|t| t.window)
            .ok_or_else(|| fail("text field not open"))?;
        for unit in text.encode_utf16() {
            unsafe {
                SendMessageW(field, WM_CHAR, Some(WPARAM(unit as usize)), None);
            }
            self.hold(frames)?;
        }
        Ok(())
    }
    fn last_bounds(&self) -> Result<Rect> {
        let objects = self.session.capture.editor.objects();
        objects
            .last()
            .map(|o| o.resolved_bounds(objects))
            .ok_or_else(|| fail("nothing drawn"))
    }
    fn write_pointers(&self) -> Result<()> {
        let pointers: String = self
            .pointers
            .iter()
            .map(|at| format!("{:.1} {:.1}\n", at.x, at.y))
            .collect();
        std::fs::write(self.folder.join("pointer.txt"), pointers).map_err(|e| fail(&e.to_string()))
    }
}

/// Shrink the circle and move it onto the legend (its bound arrow follows), then enlarge the
/// note from its corner and, as a separate gesture, tilt it.
fn film(desktop: &Image, folder: &Path) -> Result<()> {
    std::fs::create_dir_all(folder)
        .map_err(|e| Error::new(windows::Win32::Foundation::E_FAIL, e.to_string()))?;
    let mut editor = annotations();
    editor.tool = Tool::Select;
    let note = note_id(&editor)?;
    let mut film = Film {
        session: session(desktop, editor)?,
        folder: folder.to_path_buf(),
        pointers: Vec::new(),
        native_cursor: false,
        window: HWND::default(),
    };
    let drag = |e: &mut Editor, at: Point| e.update(at);
    let selection = |film: &Film| {
        film.session
            .capture
            .editor
            .selection_bounds()
            .ok_or_else(|| Error::new(windows::Win32::Foundation::E_FAIL, "nothing selected"))
    };
    // Select the circle, shrink it from its corner, then carry it onto the legend.
    film.shoot(p(760., 520.))?;
    film.hold(10)?;
    film.travel(p(600., 238.), 16, |_, _| {})?;
    film.press();
    film.release();
    film.hold(8)?;
    let corner = selection(&film)?.max;
    film.travel(corner, 14, |_, _| {})?;
    film.press();
    film.hold(3)?;
    film.travel(p(corner.x - 132., corner.y - 132.), 22, drag)?;
    film.release();
    film.hold(10)?;
    let circle = selection(&film)?;
    let grab = p(circle.center().x, circle.min.y + 2.);
    film.travel(grab, 12, |_, _| {})?;
    film.press();
    film.hold(3)?;
    let (dx, dy) = (765. - circle.center().x, 277. - circle.center().y);
    film.travel(p(grab.x + dx, grab.y + dy), 26, drag)?;
    film.release();
    film.hold(14)?;
    let label = film
        .session
        .capture
        .editor
        .objects()
        .iter()
        .find(|o| o.id == note)
        .map(|o| {
            o.resolved_bounds(film.session.capture.editor.objects())
                .center()
        })
        .ok_or_else(|| Error::new(windows::Win32::Foundation::E_FAIL, "missing note"))?;
    film.travel(label, 16, |_, _| {})?;
    film.press();
    film.release();
    film.hold(8)?;
    // Scale: drag the bottom-right corner along the diagonal, the top-left corner stays.
    let before = selection(&film)?;
    film.travel(before.max, 14, |_, _| {})?;
    film.press();
    film.hold(3)?;
    let grown = before.min + (before.max - before.min) * 1.35;
    film.travel(grown, 22, drag)?;
    film.release();
    film.hold(14)?;
    // Tilt: the rotation handle, a distinct gesture; the frame turns with the note.
    let (Some(frame), Some(handle)) = (
        film.session.capture.editor.selection_frame(),
        film.session.capture.editor.rotation_handle(),
    ) else {
        return Err(Error::new(
            windows::Win32::Foundation::E_FAIL,
            "note not selected",
        ));
    };
    film.travel(handle, 12, |_, _| {})?;
    film.press();
    film.hold(3)?;
    let frames = 22;
    for i in 1..=frames {
        let t = i as f64 / frames as f64;
        let at = turned(handle, frame.bounds.center(), -12. * t * t * (3. - 2. * t));
        film.session.capture.editor.update(at);
        film.shoot(at)?;
    }
    film.release();
    film.hold(30)?;
    film.write_pointers()
}

/// From a bare desktop to a finished capture: select an area, draw an arrow, type a note, grow
/// it with the wheel, recolour and turn it, draw a square and move it, then widen the area.
fn capture_film(desktop: &Image, folder: &Path) -> Result<()> {
    std::fs::create_dir_all(folder).map_err(|e| fail(&e.to_string()))?;
    // Hidden parent for the native text field, as the capture window hosts it.
    let owner = crate::resources::OwnedWindow(unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("STATIC"),
            w!("Site preview"),
            WS_POPUP,
            0,
            0,
            desktop.width as i32,
            desktop.height as i32,
            None,
            None,
            None,
            None,
        )?
    });
    let mut session = session::Session::new(desktop.clone())?;
    session.capture.editor.style.width = 4.;
    session.capture.text_size = 20.;
    let mut film = Film {
        session,
        folder: folder.to_path_buf(),
        pointers: Vec::new(),
        native_cursor: false,
        window: owner.0,
    };
    let none = Modifiers::default();
    let shift = Modifiers {
        shift: true,
        ..none
    };
    // Select an area that still cuts the board short.
    film.shoot(p(330., 300.))?;
    film.hold(10)?;
    film.glide(p(100., 86.), 12, none)?;
    film.down(none)?;
    film.glide(p(AREA_RIGHT, 470.), 22, none)?;
    film.up(none);
    film.hold(8)?;
    // An arrow onto the chart.
    film.pick(Action::Tool(Tool::Arrow))?;
    film.glide(p(955., 200.), 12, none)?;
    film.down(none)?;
    film.glide(p(684., 303.), 14, none)?;
    film.up(none);
    film.hold(5)?;
    // A typed note, grown with the wheel, turned white and rotated.
    film.pick(Action::Tool(Tool::Text))?;
    film.glide(p(874., 138.), 12, none)?;
    film.click()?;
    film.hold(3)?;
    film.type_text("Mobile +12%", 2)?;
    film.hold(6)?;
    film.pick(Action::Tool(Tool::Select))?;
    film.glide(film.last_bounds()?.center(), 12, none)?;
    film.click()?;
    film.hold(4)?;
    for _ in 0..3 {
        film.session.wheel(film.window, 120)?;
        film.hold(3)?;
    }
    film.hold(4)?;
    film.pick(Action::Color(WHITE))?;
    let (Some(handle), Some(bounds)) = (
        film.session.capture.rotation_handle(),
        film.session.capture.editor.selection_bounds(),
    ) else {
        return Err(fail("note not selected"));
    };
    film.glide(handle, 10, none)?;
    film.down(none)?;
    film.hold(2)?;
    let frames = 18;
    for i in 1..=frames {
        let t = i as f64 / frames as f64;
        film.step(
            turned(handle, bounds.center(), -10. * t * t * (3. - 2. * t)),
            none,
        )?;
    }
    film.up(none);
    film.hold(8)?;
    // A blue square around the chart's centre, then moved onto the moon.
    film.pick(Action::Tool(Tool::Rectangle))?;
    film.pick(Action::Color(BLUE))?;
    film.glide(p(553., 296.), 12, none)?;
    film.down(shift)?;
    film.glide(p(650., 386.), 16, shift)?;
    film.up(shift);
    film.hold(6)?;
    film.pick(Action::Tool(Tool::Select))?;
    let square = film.last_bounds()?;
    let grab = p(square.center().x, square.min.y + 2.);
    film.glide(grab, 12, none)?;
    film.click()?;
    film.hold(3)?;
    film.down(none)?;
    film.hold(2)?;
    let (dx, dy) = (406. - square.center().x, 300. - square.center().y);
    film.glide(p(grab.x + dx, grab.y + dy), 22, none)?;
    film.up(none);
    film.hold(10)?;
    // Annotations done, the area is widened from its bottom-left corner to take the whole board.
    film.glide(p(100., 470.), 16, none)?;
    film.down(none)?;
    film.hold(2)?;
    film.glide(p(100., 612.), 16, none)?;
    film.up(none);
    film.hold(45)?;
    film.write_pointers()
}

/// The eyedropper wanders a few pixels around where the moon sets behind the mountain: sky, moon
/// and ridge pass under the loupe and its colour code follows. A closed figure eight, so the
/// looping film has no jump.
fn eyedropper_film(desktop: &Image, folder: &Path) -> Result<()> {
    std::fs::create_dir_all(folder).map_err(|e| fail(&e.to_string()))?;
    let mut film = Film {
        session: session(desktop, Editor::default())?,
        folder: folder.to_path_buf(),
        pointers: Vec::new(),
        native_cursor: false,
        window: HWND::default(),
    };
    film.session.capture.eyedropper = true;
    let center = p(406., 317.);
    let frames = 150;
    for i in 0..frames {
        let t = std::f64::consts::TAU * i as f64 / frames as f64;
        film.shoot(p(center.x + 6. * t.sin(), center.y + 4. * (2. * t).sin()))?;
    }
    film.write_pointers()
}

/// Centre of the support film's heart once drawn, before it is enlarged.
const HEART: Point = Point { x: 230., y: 150. };
const THANKS: &str = "Thank you.";
/// Top of the thank-you line: it sits under the heart, on the heart's axis.
const THANKS_TOP: f64 = 295.;
/// Cupid's arrow rises to the right through this point of the enlarged heart.
const ARROW_THROUGH: Point = Point { x: 246., y: 176. };
const ARROW_SLOPE_DEGREES: f64 = 30.;
/// Visible lengths outside the heart: the tail before it goes in, the head once it is out.
const ARROW_TAIL: f64 = 84.;
const ARROW_HEAD: f64 = 78.;
/// Length of shaft seen inside the heart before the arrow crosses the outline on its way out.
const ARROW_INSIDE: f64 = 32.;

/// A self-contained thank-you film painted by the real editor, native cursors included: a thick
/// pen heart enlarged from its corner, pierced by an arrow, then signed under its axis.
/// INVARIANTS: the arrow is a line that stops on the outline, where it goes in, and an arrow that
/// starts a little way inside the far side and crosses the outline on its way out, on one axis.
/// The empty stretch between them reads as the shaft hidden in the heart.
fn support_heart_film(folder: &Path) -> Result<()> {
    std::fs::create_dir_all(folder).map_err(|e| fail(&e.to_string()))?;
    // Image pixels are stored as BGRA. This renders the site's #161b22 panel colour.
    // Sixteen actions on one row take 548 px of screen.
    let backdrop = Image::solid(560, 420, [34, 27, 22, 255]);
    // Hidden parent for the native text field, as the capture window hosts it.
    let owner = crate::resources::OwnedWindow(unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("STATIC"),
            w!("Site preview"),
            WS_POPUP,
            0,
            0,
            backdrop.width as i32,
            backdrop.height as i32,
            None,
            None,
            None,
            None,
        )?
    });
    let mut session = session::Session::new(backdrop)?;
    session.capture.region = Rect::new(p(50., 30.), p(450., 340.));
    session.capture.selecting = false;
    session.capture.editor.style.width = 14.;
    session.capture.text_size = 24.;
    let mut film = Film {
        session,
        folder: folder.to_path_buf(),
        pointers: Vec::new(),
        native_cursor: true,
        window: owner.0,
    };
    let none = Modifiers::default();
    // A new action must widen the film, not send the close button alone onto a second row.
    let buttons = film.session.capture.buttons();
    let mut bar = buttons.iter().filter(|b| b.panel == 1);
    let top = bar.next().map(|b| b.bounds.min.y);
    if bar.any(|b| Some(b.bounds.min.y) != top) {
        return Err(fail("the film is too narrow: its action bar wraps"));
    }

    film.shoot(p(280., 180.))?;
    film.hold(8)?;
    film.pick(Action::Tool(Tool::Pen))?;
    film.pick(Action::Color(RED))?;

    let heart = heart_points(HEART, 5.4, 72);
    film.glide(heart[0], 12, none)?;
    film.down(none)?;
    for point in heart.iter().copied().skip(1) {
        film.step(point, none)?;
    }
    film.up(none);
    film.hold(8)?;

    film.pick(Action::Tool(Tool::Select))?;
    film.glide(heart[18], 12, none)?;
    film.click()?;
    film.hold(5)?;
    let corner = film
        .session
        .capture
        .editor
        .selection_bounds()
        .ok_or_else(|| fail("heart was not selected"))?
        .max;
    film.glide(corner, 10, none)?;
    film.down(none)?;
    film.hold(3)?;
    film.glide(corner + p(32., 28.), 18, none)?;
    film.up(none);
    film.hold(8)?;

    // Cupid's arrow: where its axis meets the enlarged outline, on the way in and out.
    let objects = film.session.capture.editor.objects();
    let (outline, frame) = (paths(&objects[0], objects).concat(), film.last_bounds()?);
    let (sin, cos) = ARROW_SLOPE_DEGREES.to_radians().sin_cos();
    let unit = p(cos, -sin);
    let reach = frame.size().x + frame.size().y;
    let (from, to) = (ARROW_THROUGH - unit * reach, ARROW_THROUGH + unit * reach);
    let hits = crossings(&outline, from, to);
    let (Some(&entry), Some(&exit)) = (hits.first(), hits.last()) else {
        return Err(fail("the arrow misses the heart"));
    };
    if hits.len() != 2 {
        return Err(fail("the arrow must enter and leave the heart once"));
    }
    let (entry, exit) = (from + (to - from) * entry, from + (to - from) * exit);
    if (exit - entry).length() < 3. * ARROW_INSIDE {
        return Err(fail("the arrow hides no stretch inside the heart"));
    }
    // Thinner than the heart: three notches down the width ladder.
    film.pick(Action::Tool(Tool::Line))?;
    for _ in 0..3 {
        film.session.wheel(film.window, -120)?;
        film.hold(2)?;
    }
    film.glide(entry - unit * ARROW_TAIL, 12, none)?;
    film.down(none)?;
    film.glide(entry, 12, none)?;
    film.up(none);
    film.hold(5)?;
    film.pick(Action::Tool(Tool::Arrow))?;
    film.glide(exit - unit * ARROW_INSIDE, 12, none)?;
    film.down(none)?;
    film.glide(exit + unit * ARROW_HEAD, 12, none)?;
    film.up(none);
    film.hold(8)?;

    // The line is as wide as its text: start half of it left of the heart's axis.
    let width = film
        .session
        .native
        .renderer
        .measure_annotation_text(THANKS, film.session.capture.text_size)?
        .x;
    film.pick(Action::Tool(Tool::Text))?;
    film.glide(p(frame.center().x - width / 2., THANKS_TOP), 12, none)?;
    film.click()?;
    film.hold(3)?;
    film.type_text(THANKS, 2)?;
    film.hold(4)?;
    if !film.session.text_command(film.window, 13, true) {
        return Err(fail("thank-you text field did not commit"));
    }
    if (film.last_bounds()?.center().x - frame.center().x).abs() > 0.5 {
        return Err(fail("the thank-you line is off the heart's axis"));
    }
    film.hold(6)?;
    film.pick(Action::Tool(Tool::Select))?;
    film.glide(p(405., 300.), 10, none)?;
    film.click()?;
    film.hold(30)?;
    film.write_pointers()
}

fn paint(mut session: session::Session, path: &Path) -> Result<()> {
    let (width, height) = (session.capture.image.width, session.capture.image.height);
    save_png(&paint_image(width, height, |hdc| session.paint(hdc))?, path)
}

pub fn render(backdrops: &Path, out: &Path) -> Result<()> {
    let desktop = load_png(&backdrops.join("desktop-capture.png"))?;
    if (desktop.width, desktop.height) != (1100, 680) {
        return Err(Error::new(
            windows::Win32::Foundation::E_INVALIDARG,
            "desktop-capture.png must be 1100x680",
        ));
    }
    paint(session(&desktop, annotations())?, &out.join("capture.png"))?;

    eyedropper_film(&desktop, &out.join("eyedropper-frames"))?;
    film(&desktop, &out.join("editing-frames"))?;
    capture_film(&desktop, &out.join("capture-frames"))?;
    support_heart_film(&out.join("support-heart-frames"))?;

    let stage = load_png(&backdrops.join("desktop-presentation.png"))?;
    presentation::demo::render_over(&stage, &out.join("presentation.png"))?;
    presentation::demo::film_over(&stage, &out.join("presentation-frames"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_cursor_is_painted_into_the_rendered_frame() -> Result<()> {
        let pointer = p(48., 48.);
        let image = paint_image(96, 96, |hdc| {
            draw_native_cursor(
                hdc,
                windows::Win32::UI::WindowsAndMessaging::IDC_HAND,
                pointer,
            )
        })?;
        let changed = image
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[..3] != [0, 0, 0])
            .count();
        assert!(changed > 20, "the native cursor did not alter the frame");
        Ok(())
    }

    #[test]
    fn heart_path_is_closed_and_large_enough_to_edit() {
        let points = heart_points(HEART, 5.4, 72);
        assert_eq!(points.first(), points.last());
        let bounds = points.iter().fold(
            Rect {
                min: p(f64::INFINITY, f64::INFINITY),
                max: p(f64::NEG_INFINITY, f64::NEG_INFINITY),
            },
            |bounds, point| Rect {
                min: p(bounds.min.x.min(point.x), bounds.min.y.min(point.y)),
                max: p(bounds.max.x.max(point.x), bounds.max.y.max(point.y)),
            },
        );
        assert!(bounds.size().x > 150. && bounds.size().y > 140.);
        assert!(bounds.min.y < 100. && bounds.max.y > 230.);
    }

    #[test]
    fn crossings_follow_the_segment_and_ignore_what_it_misses() {
        let square = [p(10., 0.), p(30., 0.), p(30., 20.), p(10., 20.), p(10., 0.)];
        assert_eq!(
            crossings(&square, p(0., 10.), p(40., 10.)),
            [0.25, 0.75],
            "entry before exit, whatever the outline's own order"
        );
        assert_eq!(crossings(&square, p(40., 10.), p(20., 10.)), [0.5]);
        assert!(crossings(&square, p(0., 30.), p(40., 30.)).is_empty());
        assert!(crossings(&square, p(0., 0.), p(5., 0.)).is_empty());
    }

    /// The film checks its own picture: one way in, one way out, thanks on the heart's axis.
    #[test]
    fn support_heart_film_renders_a_pierced_heart_over_centred_thanks() -> Result<()> {
        let _com = ComApartment::new()?;
        let folder =
            std::env::temp_dir().join(format!("shotoshot-support-heart-{}", std::process::id()));
        let filmed = support_heart_film(&folder);
        let frames = std::fs::read_dir(&folder).map_or(0, |entries| entries.count());
        let _ = std::fs::remove_dir_all(&folder);
        filmed?;
        assert!(frames > 300, "only {frames} files for a ten-second film");
        Ok(())
    }
}
