//! ROLE: Website previews: the real capture UI, presentation aids and Board session over supplied
//! desktop pictures. The pictures come from the site repository; coordinates below match its
//! fictional desktops.
use crate::board::bake;
use crate::*;
use shotoshot_core::board::{Gathered, Screen};
use shotoshot_core::*;
use shotoshot_ui::{keys::KeyOutcome, toolbar::Action};
use std::path::{Path, PathBuf};
use windows::Win32::{
    Foundation::{HWND, WPARAM},
    Graphics::Gdi::{DeleteObject, HDC},
    UI::WindowsAndMessaging::{
        CreateWindowExW, DI_NORMAL, DrawIconEx, GetIconInfo, HCURSOR, HICON, ICONINFO,
        SendMessageW, WM_CHAR, WS_EX_TOOLWINDOW, WS_POPUP,
    },
};
use windows::core::{Error, Result, w};

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

/// Sizes of the notes: the pages show these pictures reduced, a note at the default 24 px
/// reads small there.
const NOTE_SIZE: f32 = 32.;
const LABEL_SIZE: f32 = 40.;
/// The label takes two lines: one would not fit between the board and the area's right edge.
const LABEL: &str = "Mobile\n+12%";

/// A note of the current colour, measured as the text field measures what is typed.
fn note(editor: &mut Editor, renderer: &Renderer, at: Point, text: &str, size: f32) -> Result<()> {
    let extent = renderer.measure_annotation_text(text, size)?;
    editor
        .add_text_measured_sized_colored(at, text.into(), extent, editor.style.color, size.into())
        .map(|_| ())
        .ok_or_else(|| fail("note refused"))
}

/// Annotated launch board: framed card, highlighted subtitle, circled chart, bound arrow.
fn annotations(renderer: &Renderer) -> Result<Editor> {
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
    // Low enough under the frame for the editing film to enlarge and tilt it clear of the frame.
    note(
        &mut editor,
        renderer,
        p(178., 480.),
        "Title too low?",
        NOTE_SIZE,
    )?;
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
        p(952., 232.),
        p(640., 300.),
        true,
    );
    editor.style.color = WHITE;
    note(&mut editor, renderer, p(892., 106.), LABEL, LABEL_SIZE)?;
    editor.tool = Tool::Rectangle;
    Ok(editor)
}

fn session(backdrop: &Image, editor: Editor) -> Result<session::Session> {
    let mut session = session::Session::new(backdrop.clone())?;
    session.capture.editor = editor;
    session.capture.region = Rect::new(p(100., 86.), p(AREA_RIGHT, 612.));
    session.capture.selecting = false;
    Ok(session)
}

/// The capture session over the annotated launch board.
fn annotated(backdrop: &Image) -> Result<session::Session> {
    let mut session = session(backdrop, Editor::default())?;
    session.capture.editor = annotations(&session.native.renderer)?;
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

/// Hidden parent for the native text field, as the capture window hosts it over `desktop`.
fn text_owner(desktop: &Image) -> Result<crate::resources::OwnedWindow> {
    let window = unsafe {
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
    };
    Ok(crate::resources::OwnedWindow(window))
}

fn draw_native_cursor(hdc: HDC, cursor: HCURSOR, pointer: Point) -> Result<()> {
    // PITFALL: GetIconInfo allocates two GDI bitmaps even for a shared system cursor.
    unsafe {
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
        let cursor = (self.native_cursor)
            .then(|| self.session.cursor(pointer))
            .transpose()?;
        let frame = paint_image(width, height, |hdc| {
            self.session.paint(hdc)?;
            if let Some(cursor) = cursor {
                draw_native_cursor(hdc, cursor, pointer)?;
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
    /// Frames the session does not paint: the screen before it opens or once it has ended.
    fn still(&mut self, picture: &Image, pointer: Point, frames: usize) -> Result<()> {
        for _ in 0..frames {
            save_png(
                picture,
                &self.folder.join(format!("{:03}.png", self.pointers.len())),
            )?;
            self.pointers.push(pointer);
        }
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
    let mut session = annotated(desktop)?;
    session.capture.editor.tool = Tool::Select;
    let note = note_id(&session.capture.editor)?;
    let mut film = Film {
        session,
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
        let at = turned(handle, frame.bounds.center(), -8. * t * t * (3. - 2. * t));
        film.session.capture.editor.update(at);
        film.shoot(at)?;
    }
    film.release();
    // Enlarged and tilted, the note must still sit clear of the card's frame above it.
    let objects = film.session.capture.editor.objects();
    let (Some(card), Some(text)) = (objects.first(), objects.iter().find(|o| o.id == note)) else {
        return Err(fail("missing note"));
    };
    if text.bounds().min.y < card.bounds().max.y + card.style.width {
        return Err(fail("the note runs into the card's frame"));
    }
    film.hold(30)?;
    film.write_pointers()
}

/// From a bare desktop to a finished capture: select an area, draw an arrow, type a two-line
/// note, grow it with the wheel, recolour and turn it, draw a square and move it, widen the area,
/// then come back to the arrow: its tip swings onto the square, the eyedropper recolours it and
/// the wheel doubles its stroke; the square, last, widens a little.
fn capture_film(desktop: &Image, folder: &Path) -> Result<()> {
    std::fs::create_dir_all(folder).map_err(|e| fail(&e.to_string()))?;
    let owner = text_owner(desktop)?;
    let mut session = session::Session::new(desktop.clone())?;
    session.capture.editor.style.width = 4.;
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
    // A short arrow from above the chart onto the Mobile line of its legend. Its tail stays put:
    // the film's end swings its tip to the far side, onto the moon.
    film.pick(Action::Tool(Tool::Arrow))?;
    film.glide(ARROW_TAIL_AT, 12, none)?;
    film.down(none)?;
    film.glide(p(724., 275.), 14, none)?;
    film.up(none);
    film.hold(5)?;
    // A typed note, grown with the wheel, turned white and rotated.
    film.pick(Action::Tool(Tool::Text))?;
    film.glide(p(892., 134.), 12, none)?;
    film.click()?;
    film.hold(3)?;
    // Return breaks the line in the text field, as on the keyboard.
    film.type_text(&LABEL.replace('\n', "\r"), 2)?;
    film.hold(6)?;
    film.pick(Action::Tool(Tool::Select))?;
    film.glide(film.last_bounds()?.center(), 12, none)?;
    film.click()?;
    film.hold(4)?;
    // Four notches up the size ladder, from the default 24 px.
    for _ in 0..4 {
        film.session.wheel(film.window, 120)?;
        film.hold(3)?;
    }
    if film.session.capture.text_size != LABEL_SIZE {
        return Err(fail("the wheel did not bring the note to its size"));
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
    // The handle sits above the note: too near the area's top, the size badge takes the press.
    let tilted = |o: &Object| matches!(o.shape, Shape::Text { .. }) && o.rotation != 0.;
    if !film.session.capture.editor.objects().iter().any(tilted) {
        return Err(fail("the note did not turn"));
    }
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
    film.hold(12)?;
    // Back to the first arrow: only its tip handle is dragged, well past a right angle, onto the
    // square now on the moon. The shaft lengthens; tail, thickness and head stay as drawn.
    let drawn = arrow_of(&film)?;
    film.glide(drawn.middle(), 14, none)?;
    film.click()?;
    film.hold(8)?;
    film.glide(drawn.end, 10, none)?;
    film.down(none)?;
    film.hold(3)?;
    let square = film.last_bounds()?;
    // A quarter slower than the other drags: the swing is the point of the film.
    film.glide(p(square.max.x + 9., square.center().y), 33, none)?;
    film.up(none);
    film.hold(10)?;
    let aimed = arrow_of(&film)?;
    if !aimed.reaimed_from(&drawn) {
        return Err(fail(
            "the arrow's tip did not swing past a right angle alone",
        ));
    }
    // The eyedropper recolours the arrow still selected: picking it, unlike a tool, keeps the
    // selection. The sample is the chart's green slice.
    film.pick(Action::Eyedropper)?;
    film.glide(p(532., 335.), 22, none)?;
    film.hold(10)?;
    film.click()?;
    film.hold(10)?;
    let sampled = film.session.capture.custom_color;
    let recoloured = arrow_of(&film)?;
    if recoloured.color != sampled || sampled == drawn.color {
        return Err(fail("the eyedropper did not recolour the selected arrow"));
    }
    // The wheel over the arrow, still selected, doubles its stroke: shaft and head grow together.
    film.glide(recoloured.middle(), 14, none)?;
    film.hold(4)?;
    for _ in 0..4 {
        film.session.wheel(film.window, 120)?;
        film.hold(5)?;
    }
    film.hold(8)?;
    let grown = arrow_of(&film)?;
    if grown.width != 2. * drawn.width || grown.color != sampled {
        return Err(fail("the wheel did not double the arrow alone"));
    }
    // The square, selected in turn, only widens a little and stays blue.
    let square = film.last_bounds()?;
    film.glide(p(square.center().x, square.min.y + 2.), 14, none)?;
    film.click()?;
    film.hold(4)?;
    for _ in 0..2 {
        film.session.wheel(film.window, 120)?;
        film.hold(5)?;
    }
    film.hold(8)?;
    let widened = film
        .session
        .capture
        .editor
        .objects()
        .last()
        .map(|o| o.style);
    if widened
        != Some(Style {
            color: BLUE,
            width: 1.5 * drawn.width,
        })
    {
        return Err(fail("the wheel did not widen the blue square a little"));
    }
    // A click on the bare board drops the selection: the last frames show the capture as saved.
    film.glide(p(760., 505.), 16, none)?;
    film.click()?;
    if !film.session.capture.editor.selected.is_empty() {
        return Err(fail("the closing click left something selected"));
    }
    film.hold(45)?;
    film.write_pointers()
}

/// Where the hero film's arrow starts, above the chart, and stays.
const ARROW_TAIL_AT: Point = Point { x: 640., y: 200. };

/// The one arrow of the hero film, as its gestures read it.
struct Aim {
    start: Point,
    end: Point,
    color: [u8; 3],
    width: f64,
}
impl Aim {
    fn middle(&self) -> Point {
        self.start + (self.end - self.start) * 0.5
    }
    /// Same tail and stroke, a longer shaft turned more than 90°.
    fn reaimed_from(&self, before: &Aim) -> bool {
        let (was, is) = (before.end - before.start, self.end - self.start);
        let cosine = (was.x * is.x + was.y * is.y) / (was.length() * is.length());
        self.start == before.start
            && self.width == before.width
            && is.length() > was.length()
            && cosine < 0.
    }
}

fn arrow_of(film: &Film) -> Result<Aim> {
    film.session
        .capture
        .editor
        .objects()
        .iter()
        .find_map(|o| match o.shape {
            Shape::Arrow { start, end, .. } => Some(Aim {
                start,
                end,
                color: o.style.color,
                width: o.style.width,
            }),
            _ => None,
        })
        .ok_or_else(|| fail("missing arrow"))
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
const THANKS_TOP: f64 = 286.;
const THANKS_SIZE: f32 = 36.;
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
    let owner = text_owner(&backdrop)?;
    let mut session = session::Session::new(backdrop)?;
    session.capture.region = Rect::new(p(50., 30.), p(450., 340.));
    session.capture.selecting = false;
    session.capture.editor.style.width = 14.;
    session.capture.text_size = THANKS_SIZE;
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

/// The desktop of the desktop-notes film, as Windows stacks it. The three pictures share one
/// size.
struct NotesDesktop {
    /// The user's own wallpaper: what the notes are baked into.
    wallpaper: Image,
    /// What Explorer draws over the wallpaper, icons and labels alone: premultiplied, with
    /// their own alpha.
    icons: Image,
    /// The screen at rest: the wallpaper under its icons and its taskbar.
    screen: Image,
}

/// A zone of the tidy desktop: drawn around one group of icons, filled, titled over them.
struct Zone {
    area: Rect,
    /// A free spot between the group's icons, where the bucket fills the zone.
    gap: Point,
    title: &'static str,
    title_at: Point,
}
const ZONES: [Zone; 2] = [
    Zone {
        area: Rect {
            min: Point { x: 106., y: 14. },
            max: Point { x: 284., y: 214. },
        },
        gap: Point { x: 195., y: 128. },
        title: "PERSONAL",
        title_at: Point { x: 118., y: 18. },
    },
    Zone {
        area: Rect {
            min: Point { x: 306., y: 14. },
            max: Point { x: 484., y: 214. },
        },
        gap: Point { x: 395., y: 128. },
        title: "WORK",
        title_at: Point { x: 318., y: 18. },
    },
];
const TODO: &str = "Today\n- send the build\n- call Sam at 3";
const TODO_AT: Point = Point { x: 560., y: 30. };
/// The step the pointer takes off a note it has just opened: the first letter shows.
const ASIDE: Point = Point { x: -26., y: 30. };
/// The arrow leaves the to-do's second line for the right edge of the Builds folder, in the
/// work zone.
const ARROW_FROM: Point = Point { x: 550., y: 84. };
const ARROW_TO: Point = Point { x: 462., y: 78. };
/// Where the pointer waits on the first frame and once the notes are drawn.
const REST: Point = Point { x: 620., y: 360. };
const DESKTOP_NOTE_SIZE: f32 = 26.;
/// Icon pixels that must lie over a note, about half of one icon: fewer would not read as an
/// icon in front of what was drawn.
const MIN_ICON_PIXELS: usize = 500;

fn bgra(image: &Image) -> &[[u8; 4]] {
    image.pixels.as_chunks::<4>().0
}

/// Indices of the pixels of `area` in a picture of `width` by `height`, clipped to it.
fn pixels_in(area: Rect, width: u32, height: u32) -> impl Iterator<Item = usize> {
    let (width, height) = (width as usize, height as usize);
    let span = |from: f64, to: f64, limit: usize| {
        from.clamp(0., limit as f64) as usize..to.ceil().clamp(0., limit as f64) as usize
    };
    let columns = span(area.min.x, area.max.x, width);
    span(area.min.y, area.max.y, height)
        .flat_map(move |y| y * width + columns.start..y * width + columns.end)
}

/// Pixels of `area` that `a` and `b`, two pictures of one size, do not share.
fn differing(a: &Image, b: &Image, area: Rect) -> usize {
    pixels_in(area, a.width, a.height)
        .filter(|&at| bgra(a)[at][..3] != bgra(b)[at][..3])
        .count()
}

/// Pixels of `area` that `layer` covers whole.
fn opaque(layer: &Image, area: Rect) -> usize {
    pixels_in(area, layer.width, layer.height)
        .filter(|&at| bgra(layer)[at][3] == 255)
        .count()
}

/// Straight BGRA, as a PNG is read, to the premultiplied BGRA a renderer lays over its notes.
fn premultiplied(mut image: Image) -> Image {
    for pixel in image.pixels.as_chunks_mut::<4>().0 {
        let alpha = pixel[3] as u32;
        for channel in &mut pixel[..3] {
            *channel = ((*channel as u32 * alpha + 127) / 255) as u8;
        }
    }
    image
}

/// `base`, opaque, under the premultiplied `layer` of the same size.
fn under(base: &Image, layer: &Image) -> Image {
    let mut out = base.clone();
    for (pixel, over) in out
        .pixels
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(bgra(layer))
    {
        let rest = 255 - over[3] as u32;
        for (channel, ink) in pixel[..3].iter_mut().zip(over) {
            *channel = (*ink as u32 + (*channel as u32 * rest + 127) / 255).min(255) as u8;
        }
    }
    out
}

/// Pixels where `frame` shows `icons` whole although a note is drawn there: `baked` holds the
/// notes, `wallpaper` does not.
fn kept_in_front(frame: &Image, icons: &Image, baked: &Image, wallpaper: &Image) -> usize {
    bgra(frame)
        .iter()
        .zip(bgra(icons))
        .zip(bgra(baked).iter().zip(bgra(wallpaper)))
        .filter(|((shown, icon), (note, bare))| {
            icon[3] == 255 && note[..3] != bare[..3] && shown[..3] == icon[..3]
        })
        .count()
}

/// The Board, Shotoshot's notes on the desktop: from a calm screen, a session opens on Explorer's
/// rendering (the taskbar is a window: it goes), two zones are drawn around groups of icons,
/// filled and titled, a to-do is typed and an arrow points from it at a folder. Escape ends the
/// session; `baked.png` is the wallpaper Shotoshot then hands Windows, notes included. What the
/// closed desktop does over it (icons, taskbar, a window, an icon dragged across the notes) is
/// the site's to stack: it owns that fiction.
/// INVARIANTS: every note is made by the session's own gestures; `baked.png` is the bake's own
/// picture.
/// PITFALL: a session lifts the icons off the screen, opaque. The film hands it the drawn
/// desktop's own layer instead, with its alpha: lifted, the soft rim of those icons keeps the
/// dark wallpaper it was blended with and rings them on a light zone, which the shadowed labels
/// of a real desktop do not show.
fn desktop_notes_film(desktop: &NotesDesktop, folder: &Path) -> Result<()> {
    let NotesDesktop {
        wallpaper,
        icons,
        screen,
    } = desktop;
    let size = |image: &Image| (image.width, image.height);
    if size(wallpaper) != size(screen) || size(icons) != size(screen) {
        return Err(fail("the desktop's three pictures differ in size"));
    }
    let whole = Rect {
        min: p(0., 0.),
        max: p(screen.width as f64, screen.height as f64),
    };
    let screens = [Screen {
        id: "film".into(),
        bounds: whole,
    }];
    if ZONES
        .iter()
        .any(|zone| opaque(icons, zone.area) < MIN_ICON_PIXELS)
    {
        return Err(fail("a zone is drawn around no icon"));
    }
    std::fs::create_dir_all(folder).map_err(|e| fail(&e.to_string()))?;
    let owner = text_owner(screen)?;
    let mut session = session::Session::board(
        under(wallpaper, icons),
        Some(icons.clone()),
        Gathered::default(),
        screens.to_vec(),
        folder.to_path_buf(),
    )?;
    // PITFALL: a Board session lays its tools out on this machine's monitors, at their scale.
    // The film's frame is its own screen, at 100 %.
    session.capture.monitors.clear();
    // Blue is the colour an earlier session left, as the settings keep it.
    session.capture.editor.style = Style {
        color: BLUE,
        width: 4.,
    };
    session.capture.text_size = DESKTOP_NOTE_SIZE;
    let mut film = Film {
        session,
        folder: folder.to_path_buf(),
        pointers: Vec::new(),
        native_cursor: false,
        window: owner.0,
    };
    let none = Modifiers::default();
    // A calm screen, then Alt+B: the session covers it, taskbar included.
    film.still(screen, REST, 30)?;
    film.shoot(REST)?;
    film.hold(12)?;
    // The zones: the rectangle is the tool in hand when a session opens.
    for zone in &ZONES {
        film.glide(zone.area.min, 12, none)?;
        film.down(none)?;
        film.glide(zone.area.max, 14, none)?;
        film.up(none);
        film.hold(4)?;
    }
    // The bucket stays in hand from one zone to the next.
    film.pick(Action::Fill)?;
    for zone in &ZONES {
        film.glide(zone.gap, 10, none)?;
        film.click()?;
        film.hold(6)?;
    }
    // Titles and to-do, in white. Picking the text tool puts the bucket down; a click elsewhere
    // settles the note being typed and opens the next.
    film.pick(Action::Tool(Tool::Text))?;
    film.pick(Action::Color(WHITE))?;
    let written = ZONES
        .iter()
        .map(|zone| (zone.title_at, zone.title))
        .chain([(TODO_AT, TODO)]);
    for (at, text) in written {
        film.glide(at, 12, none)?;
        film.click()?;
        film.glide(at + ASIDE, 5, none)?;
        // Return breaks the line in the text field, as on the keyboard.
        film.type_text(&text.replace('\n', "\r"), 2)?;
        film.hold(6)?;
    }
    // Picking the arrow settles the to-do. ORDER: the arrow comes last, so that nothing is
    // pending and one Escape ends the session.
    film.pick(Action::Tool(Tool::Arrow))?;
    film.glide(ARROW_FROM, 12, none)?;
    film.down(none)?;
    film.glide(ARROW_TO, 14, none)?;
    film.up(none);
    film.hold(6)?;
    film.glide(REST, 12, none)?;
    film.hold(8)?;

    fn typed(note: &Object) -> Option<&str> {
        match &note.shape {
            Shape::Text { text, .. } => Some(text),
            _ => None,
        }
    }
    let objects = film.session.capture.editor.objects();
    // The text field breaks its lines with a carriage return and a line feed.
    let texts = objects
        .iter()
        .filter_map(typed)
        .map(|t| t.replace('\r', ""));
    if texts.ne(ZONES.iter().map(|zone| zone.title).chain([TODO])) || objects.len() != 6 {
        return Err(fail("the notes are not the six the film draws"));
    }
    if objects[..ZONES.len()]
        .iter()
        .any(|zone| zone.fill.is_none())
    {
        return Err(fail("the bucket did not fill a zone"));
    }
    // Both panels of the tools show whole and clear of every note.
    let panels = shotoshot_ui::toolbar::panels(&film.session.capture.buttons(), 1.);
    if panels.len() != 2 || panels.iter().any(|panel| !whole.contains_rect(*panel)) {
        return Err(fail("the toolbar is off the screen"));
    }
    let bounds = |o: &Object| o.resolved_bounds(objects);
    if objects
        .iter()
        .any(|o| panels.iter().any(|panel| bounds(o).intersects(*panel)))
    {
        return Err(fail("a note runs under the toolbar"));
    }
    let editing = paint_image(screen.width, screen.height, |hdc| film.session.paint(hdc))?;
    // Escape as the window forwards it, then what the session's owner does: the notes are baked
    // into the wallpaper.
    let (capture, mut host) = film.session.host(film.window);
    if capture.key(&mut host, 27, none) != KeyOutcome::Close {
        return Err(fail("Escape did not end the session"));
    }
    let editor = &film.session.capture.editor;
    let notes = Gathered {
        objects: editor.objects().to_vec(),
        last_id: editor.last_id(),
        background: editor.background(),
    };
    let renderer = Renderer::with(Backend::Software, false)?;
    let baked = bake::render_screens(&renderer, wallpaper, &notes, &screens)?
        .pop()
        .map(|(_, picture)| picture)
        .ok_or_else(|| fail("nothing was baked"))?;
    // The film's point, read in its own pictures: icons lie over a note while it is edited,
    // and every note is in the wallpaper the session leaves.
    if kept_in_front(&editing, icons, &baked, wallpaper) < MIN_ICON_PIXELS {
        return Err(fail("no icon lies over a note in the session"));
    }
    if notes
        .objects
        .iter()
        .any(|o| differing(&baked, wallpaper, o.resolved_bounds(&notes.objects)) == 0)
    {
        return Err(fail("a note did not reach the wallpaper"));
    }
    save_png(&baked, &folder.join("baked.png"))?;
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
    paint(annotated(&desktop)?, &out.join("capture.png"))?;

    eyedropper_film(&desktop, &out.join("eyedropper-frames"))?;
    film(&desktop, &out.join("editing-frames"))?;
    capture_film(&desktop, &out.join("capture-frames"))?;
    support_heart_film(&out.join("support-heart-frames"))?;

    let stage = load_png(&backdrops.join("desktop-presentation.png"))?;
    presentation::demo::render_over(&stage, &out.join("presentation.png"))?;
    presentation::demo::film_over(&stage, &out.join("presentation-frames"))?;

    let layer = |name: &str| load_png(&backdrops.join(format!("desktop-board-{name}.png")));
    let notes = NotesDesktop {
        wallpaper: layer("wallpaper")?,
        icons: premultiplied(layer("icons")?),
        screen: layer("screen")?,
    };
    desktop_notes_film(&notes, &out.join("board-frames"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_cursor_is_painted_into_the_rendered_frame() -> Result<()> {
        let pointer = p(48., 48.);
        let image = paint_image(96, 96, |hdc| {
            use windows::Win32::UI::WindowsAndMessaging::{IDC_HAND, LoadCursorW};
            draw_native_cursor(hdc, unsafe { LoadCursorW(None, IDC_HAND)? }, pointer)
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

    /// The other films check theirs as well: the hero's typed note reaches its size and turns,
    /// its arrow swings by its tip alone and takes the eyedropper's colour, the editing film's
    /// note stays clear of the card's frame. Any desktop will do.
    #[test]
    fn films_of_the_board_render_their_enlarged_notes() -> Result<()> {
        let _com = ComApartment::new()?;
        let desktop = Image::solid(1100, 680, [58, 22, 8, 255]);
        let folder =
            std::env::temp_dir().join(format!("shotoshot-board-films-{}", std::process::id()));
        let filmed = capture_film(&desktop, &folder.join("capture"))
            .and_then(|()| film(&desktop, &folder.join("editing")));
        let _ = std::fs::remove_dir_all(&folder);
        filmed
    }

    fn block(image: &mut Image, x: std::ops::Range<usize>, y: std::ops::Range<usize>, to: [u8; 4]) {
        let width = image.width as usize;
        for row in y {
            let pixels = &mut image.pixels[(row * width + x.start) * 4..(row * width + x.end) * 4];
            pixels.as_chunks_mut::<4>().0.fill(to);
        }
    }

    #[test]
    fn an_icon_counts_as_kept_in_front_only_over_a_note_and_where_the_frame_shows_it() {
        const ICON: [u8; 4] = [10, 200, 250, 255];
        let wallpaper = Image::solid(5, 1, [90, 40, 20, 255]);
        // Notes on the last four pixels, icons on the first three.
        let mut baked = wallpaper.clone();
        block(&mut baked, 1..5, 0..1, [92, 79, 237, 255]);
        let mut icons = Image::solid(5, 1, [0, 0, 0, 0]);
        block(&mut icons, 0..3, 0..1, ICON);
        // A frame that lets the note through on the third pixel.
        let mut frame = baked.clone();
        block(&mut frame, 0..2, 0..1, ICON);
        assert_eq!(kept_in_front(&frame, &icons, &baked, &wallpaper), 1);
        let mut desktop = frame.clone();
        block(&mut desktop, 2..3, 0..1, ICON);
        assert_eq!(kept_in_front(&desktop, &icons, &baked, &wallpaper), 2);
        assert_eq!(kept_in_front(&wallpaper, &icons, &baked, &wallpaper), 0);

        let area = |from: f64, to: f64| Rect {
            min: p(from, 0.),
            max: p(to, 1.),
        };
        assert_eq!(differing(&desktop, &wallpaper, area(0., 5.)), 5);
        assert_eq!(differing(&baked, &wallpaper, area(0., 2.)), 1);
        assert_eq!(differing(&baked, &wallpaper, area(-3., 0.5)), 0);
        assert_eq!(
            differing(&baked, &wallpaper, area(3., 40.)),
            2,
            "clipped to the picture"
        );
    }

    #[test]
    fn a_layer_is_premultiplied_then_laid_over_its_base() {
        let base = Image::solid(3, 1, [200, 100, 0, 255]);
        let mut layer = Image::solid(3, 1, [0, 0, 0, 0]);
        block(&mut layer, 1..2, 0..1, [100, 200, 50, 128]);
        block(&mut layer, 2..3, 0..1, [10, 20, 30, 255]);
        let layer = premultiplied(layer);
        assert_eq!(
            bgra(&layer),
            [[0; 4], [50, 100, 25, 128], [10, 20, 30, 255]]
        );
        let shown = under(&base, &layer);
        assert_eq!(
            bgra(&shown),
            [[200, 100, 0, 255], [150, 150, 25, 255], [10, 20, 30, 255]],
            "bare, half and half, covered"
        );
        let all = Rect {
            min: p(0., 0.),
            max: p(3., 1.),
        };
        assert_eq!(
            opaque(&layer, all),
            1,
            "half an icon is not an icon in front"
        );
    }

    /// A plain desktop of the site's size under a taskbar, one icon at `top` in each of the
    /// columns `lefts`.
    fn notes_desktop(lefts: &[usize], top: usize) -> NotesDesktop {
        let wallpaper = Image::solid(960, 540, [58, 22, 8, 255]);
        let mut icons = Image::solid(960, 540, [0, 0, 0, 0]);
        for &left in lefts {
            block(
                &mut icons,
                left..left + 40,
                top..top + 40,
                [92, 208, 255, 255],
            );
        }
        let mut screen = under(&wallpaper, &icons);
        block(&mut screen, 0..960, 496..540, [38, 28, 24, 255]);
        NotesDesktop {
            wallpaper,
            icons,
            screen,
        }
    }

    /// The desktop-notes film checks its pictures too: an icon over a note in the session,
    /// every note in the wallpaper it leaves. A zone drawn around no icon is refused unfilmed.
    #[test]
    fn desktop_notes_film_keeps_an_icon_over_each_zone_and_refuses_a_zone_without_one() -> Result<()>
    {
        let _com = ComApartment::new()?;
        let folder =
            std::env::temp_dir().join(format!("shotoshot-desktop-notes-{}", std::process::id()));
        // An icon in the first zone alone.
        assert!(desktop_notes_film(&notes_desktop(&[134], 56), &folder).is_err());
        assert!(!folder.exists(), "a refused film writes nothing");
        let filmed = desktop_notes_film(&notes_desktop(&[134, 334], 56), &folder);
        let frames = std::fs::read_dir(&folder).map_or(0, |entries| entries.count());
        let baked = load_png(&folder.join("baked.png"));
        let _ = std::fs::remove_dir_all(&folder);
        filmed?;
        assert!(
            frames > 300,
            "only {frames} files for a seven-second session"
        );
        let (baked, wallpaper) = (baked?, Image::solid(960, 540, [58, 22, 8, 255]));
        for zone in &ZONES {
            assert!(
                differing(&baked, &wallpaper, zone.area) > 20_000,
                "the wallpaper the session leaves holds the filled zone {}",
                zone.title
            );
        }
        Ok(())
    }
}
