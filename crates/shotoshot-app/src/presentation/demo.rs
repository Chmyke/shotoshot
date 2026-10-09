//! ROLE: Produces a synthetic preview using the actual presentation renderers.
use super::*;
use crate::settings::InputLayout;
use shotoshot_core::{Object, Rect, Shape, Style};

/// Where the lock sits on the full diagram the previews show.
const LOCK: (i32, i32) = diagram::Plan::FULL.lock();

/// Real presentation layers: shapes (with opacity), halo, device diagram and its lock.
struct Layers {
    shapes: Layer,
    halo: Layer,
    keys: Layer,
    lock: Layer,
    devices: Devices,
}

fn layers(width: i32, height: i32, shapes: [(Shape, f32); 3]) -> Result<Layers> {
    let shape_layer = Layer::new(std::ptr::null(), -32000, -32000, width, height)?;
    let halo = Layer::new(std::ptr::null(), -32000, -32000, 72, 72)?;
    let lock = Layer::new(
        std::ptr::null(),
        -32000,
        -32000,
        diagram::LOCK_SIZE,
        diagram::LOCK_SIZE,
    )?;
    let keys = Layer::new(
        std::ptr::null(),
        -32000,
        -32000,
        diagram::WIDTH,
        diagram::HEIGHT,
    )?;
    shape_layer.begin(false)?;
    for (shape, opacity) in shapes {
        shape_layer.object(
            &Object {
                rotation: 0.,
                fill: None,
                id: 0,
                shape,
                style: Style {
                    width: PresentationSettings::default().stroke_width as f64,
                    ..Style::default()
                },
            },
            opacity,
            PresentationSettings::default().arrow_size as f64,
        );
    }
    shape_layer.end()?;
    halo.begin(false)?;
    halo.halo(PresentationSettings::default().halo_color, false, None);
    halo.end()?;
    keys.begin(false)?;
    let mut devices = Devices::default();
    for scan in [0x1d, 0x2a, 0x38, 0x39, 0x1c, 0x45, 0x47, 0x3a, 0x0e] {
        devices.keyboard(scan, false, true, 0);
    }
    devices.keyboard(0x2e, false, true, 0);
    devices.set_label(0x2e, false, &[67]);
    devices.keyboard(83, true, true, 0);
    devices.mouse(1, 0);
    keys.devices(&devices, 0);
    keys.end()?;
    lock.begin(true)?;
    lock.lock_icon(false);
    lock.end()?;
    Ok(Layers {
        shapes: shape_layer,
        halo,
        keys,
        lock,
        devices,
    })
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Rect {
    Rect {
        min: Point { x: x0, y: y0 },
        max: Point { x: x1, y: y1 },
    }
}

fn arrow(x0: f64, y0: f64, x1: f64, y1: f64) -> Shape {
    Shape::Arrow {
        start: Point { x: x0, y: y0 },
        end: Point { x: x1, y: y1 },
        start_attach: None,
        end_attach: None,
        head: true,
    }
}

pub(crate) fn render() -> Result<()> {
    let Layers {
        shapes,
        halo,
        keys,
        lock,
        mut devices,
    } = layers(
        960,
        540,
        [
            (Shape::Rectangle(rect(85., 148., 365., 330.)), 1.),
            (arrow(402., 300., 575., 206.), 1.),
            (Shape::Ellipse(rect(610., 142., 823., 310.)), 0.4),
        ],
    )?;
    let renderer = crate::Renderer::new()?;
    let image = crate::paint_image(960, 540, |dc| {
        unsafe {
            renderer.dc.BindDC(
                dc,
                &RECT {
                    right: 960,
                    bottom: 540,
                    ..Default::default()
                },
            )?;
            renderer.dc.BeginDraw();
            renderer
                .dc
                .Clear(Some(&crate::render::color([245, 247, 250], 1.)));
        }
        for (text, x, y, size, rgb) in [
            ("Present live", 48., 30., 28., [28, 35, 48]),
            (
                "Drawing complete: applications already respond to clicks.",
                48.,
                77.,
                18.,
                [85, 96, 112],
            ),
            ("Rectangle", 135., 211., 21., [85, 96, 112]),
            ("Shape\nfading away", 641., 201., 19., [85, 96, 112]),
            ("Halo", 366., 365., 18., [85, 96, 112]),
        ] {
            renderer.text(
                &renderer.dc,
                text,
                Rect {
                    min: Point { x, y },
                    max: Point { x: 935., y: 535. },
                },
                size,
                rgb,
            )?;
        }
        unsafe {
            renderer.dc.EndDraw(None, None)?;
        }
        shapes.composite(dc, 0, 0)?;
        halo.composite(dc, 276, 341)?;
        keys.composite(dc, 220, 396)?;
        lock.composite(dc, 220 + LOCK.0, 396 + LOCK.1)
    })?;
    crate::save_png(
        &image,
        std::path::Path::new("target/shotoshot-presentation.png"),
    )?;
    devices.ansi = true;
    keys.begin(false)?;
    keys.devices(&devices, 0);
    keys.end()?;
    let american = crate::paint_image(diagram::WIDTH as u32, diagram::HEIGHT as u32, |dc| {
        keys.composite(dc, 0, 0)
    })?;
    crate::save_png(
        &american,
        std::path::Path::new("target/shotoshot-keyboard-ansi.png"),
    )?;
    render_plans()
}

/// Reference picture of what the menu can ask for: each layout, a lone keyboard, a lone
/// mouse, the ANSI left-hand block and a larger size, every one with its lock in place.
fn render_plans() -> Result<()> {
    let plan = |layout, keyboard, mouse| diagram::Plan {
        layout,
        keyboard,
        mouse,
    };
    let mut devices = Devices::default();
    // Shift, Space and the key AZERTY labels Z; right Ctrl is held but outside the block.
    for (scan, extended) in [(0x2a, false), (0x39, false), (0x11, false), (0x1d, true)] {
        devices.keyboard(scan, extended, true, 0);
    }
    devices.set_label(0x11, false, &[b'Z' as u16]);
    devices.mouse(1, 0);
    let mut drawn = Vec::new();
    for (plan, ansi, scale, x, y) in [
        (plan(InputLayout::Full, true, false), false, 1., 20, 20),
        (plan(InputLayout::Full, false, true), false, 1., 480, 20),
        (plan(InputLayout::Compact, true, true), false, 1., 20, 180),
        (plan(InputLayout::Gaming, true, false), true, 1., 480, 180),
        (plan(InputLayout::Gaming, true, true), false, 1., 20, 340),
        (plan(InputLayout::Gaming, true, false), false, 1.5, 280, 340),
    ] {
        devices.ansi = ansi;
        let keys = Layer::diagram(std::ptr::null(), plan, scale)?;
        keys.begin(false)?;
        keys.devices(&devices, 0);
        keys.end()?;
        let size = (diagram::LOCK_SIZE as f64 * scale) as i32;
        let lock = Layer::new(std::ptr::null(), -32000, -32000, size, size)?;
        lock.begin(true)?;
        lock.lock_icon(false);
        lock.end()?;
        let at = plan.lock();
        let offset = |logical: i32| (logical as f64 * scale) as i32;
        drawn.push((keys, x, y));
        drawn.push((lock, x + offset(at.0), y + offset(at.1)));
    }
    let renderer = crate::Renderer::new()?;
    let image = crate::paint_image(640, 570, |dc| {
        unsafe {
            renderer.dc.BindDC(
                dc,
                &RECT {
                    right: 640,
                    bottom: 570,
                    ..Default::default()
                },
            )?;
            renderer.dc.BeginDraw();
            renderer
                .dc
                .Clear(Some(&crate::render::color([245, 247, 250], 1.)));
            renderer.dc.EndDraw(None, None)?;
        }
        drawn
            .iter()
            .try_for_each(|(layer, x, y)| layer.composite(dc, *x, *y))
    })?;
    crate::save_png(
        &image,
        std::path::Path::new("target/shotoshot-keyboard-layouts.png"),
    )
}

/// Site preview: the same layers composited over a desktop picture (see tools in the site repo).
pub(crate) fn render_over(backdrop: &crate::Image, path: &std::path::Path) -> Result<()> {
    let (width, height) = (backdrop.width as i32, backdrop.height as i32);
    let Layers {
        shapes,
        halo,
        keys,
        lock,
        ..
    } = layers(
        width,
        height,
        [
            (Shape::Rectangle(rect(572., 136., 834., 300.)), 1.),
            (arrow(350., 130., 556., 176.), 1.),
            (Shape::Ellipse(rect(138., 226., 562., 314.)), 0.4),
        ],
    )?;
    let image = crate::paint_image(width as u32, height as u32, |dc| {
        unsafe {
            SetDIBitsToDevice(
                dc,
                0,
                0,
                width as u32,
                height as u32,
                0,
                0,
                0,
                height as u32,
                backdrop.pixels.as_ptr().cast(),
                &crate::ImageExt::info(backdrop),
                DIB_RGB_COLORS,
            );
        }
        shapes.composite(dc, 0, 0)?;
        halo.composite(dc, 668, 184)?;
        keys.composite(dc, 410, 352)?;
        lock.composite(dc, 410 + LOCK.0, 352 + LOCK.1)
    })?;
    crate::save_png(&image, path)
}

/// Frame-by-frame run of the live tools over a desktop picture: shortcut, drag, fade.
/// The layers, scene, key diagram and halo are the ones the resident controller paints.
struct LiveFilm<'a> {
    backdrop: &'a crate::Image,
    folder: std::path::PathBuf,
    preferences: PresentationSettings,
    canvas: Layer,
    halo: Layer,
    keys: Layer,
    lock: Layer,
    scene: model::Scene,
    devices: Devices,
    halo_state: halo::HaloState,
    now: u64,
    pointer: Point,
    pointers: Vec<Point>,
    recording: bool,
}
const FRAME_MS: u64 = 40;
const HALO_SIZE: i32 = 72;
const KEYS_AT: (i32, i32) = (410, 352);
const LEFT_DOWN: u16 = 0x1;
const LEFT_UP: u16 = 0x2;
const ALT: u16 = 0x38;
impl LiveFilm<'_> {
    fn pointer(&self) -> Point {
        self.pointer
    }
    fn shoot(&mut self, at: Point) -> Result<()> {
        self.scene.expire(self.now);
        self.canvas.begin(self.scene.intercepts_input())?;
        for shape in &self.scene.shapes {
            self.canvas
                .object(&shape.object, shape.opacity(self.now), shape.arrow_size);
        }
        if let Some(draft) = &self.scene.draft {
            self.canvas.object(draft, 1., self.scene.arrow_size);
        }
        self.canvas.end()?;
        self.halo.begin(false)?;
        self.halo.halo(
            self.preferences.halo_color,
            self.halo_state.pressed(),
            self.halo_state
                .needs_animation(self.now)
                .then(|| self.halo_state.pulse(self.now)),
        );
        self.halo.end()?;
        self.keys.begin(false)?;
        self.keys.devices(&self.devices, self.now);
        self.keys.end()?;
        let backdrop = self.backdrop;
        let (width, height) = (backdrop.width, backdrop.height);
        let image = crate::paint_image(width, height, |dc| {
            unsafe {
                SetDIBitsToDevice(
                    dc,
                    0,
                    0,
                    width,
                    height,
                    0,
                    0,
                    0,
                    height,
                    backdrop.pixels.as_ptr().cast(),
                    &crate::ImageExt::info(backdrop),
                    DIB_RGB_COLORS,
                );
            }
            self.canvas.composite(dc, 0, 0)?;
            self.halo
                .composite(dc, at.x as i32 - HALO_SIZE / 2, at.y as i32 - HALO_SIZE / 2)?;
            self.keys.composite(dc, KEYS_AT.0, KEYS_AT.1)?;
            self.lock
                .composite(dc, KEYS_AT.0 + LOCK.0, KEYS_AT.1 + LOCK.1)
        })?;
        if self.recording {
            let name = format!("{:03}.png", self.pointers.len());
            crate::save_png(&image, &self.folder.join(name))?;
            self.pointers.push(at);
        }
        self.pointer = at;
        self.now += FRAME_MS;
        Ok(())
    }
    fn hold(&mut self, frames: usize) -> Result<()> {
        (0..frames).try_for_each(|_| self.shoot(self.pointer()))
    }
    /// Eased pointer travel; while a shape is being drawn, its draft follows the pointer.
    fn glide(&mut self, to: Point, frames: usize) -> Result<()> {
        let from = self.pointer();
        for i in 1..=frames {
            let t = i as f64 / frames as f64;
            let t = t * t * (3. - 2. * t);
            let at = Point {
                x: from.x + (to.x - from.x) * t,
                y: from.y + (to.y - from.y) * t,
            };
            self.scene.update(at, false, false);
            if let Some(draft) = &mut self.scene.draft {
                draft.style.color = self.preferences.shape_color;
            }
            self.shoot(at)?;
        }
        Ok(())
    }
    /// A direct shortcut (Alt + letter) as the keyboard hook sees it, then the tool it arms.
    fn shortcut(&mut self, letter: u16, label: char, tool: model::LiveTool) -> Result<()> {
        self.devices.keyboard(ALT, false, true, self.now);
        self.hold(3)?;
        self.devices.keyboard(letter, false, true, self.now);
        self.devices.set_label(letter, false, &[label as u16]);
        self.scene.arm(tool);
        self.hold(6)?;
        self.devices.keyboard(letter, false, false, self.now);
        self.hold(2)?;
        self.devices.keyboard(ALT, false, false, self.now);
        self.hold(3)
    }
    fn press(&mut self) {
        let at = self.pointer();
        self.devices.mouse(LEFT_DOWN, self.now);
        self.halo_state.mouse(LEFT_DOWN, self.now);
        self.scene.begin(at);
        if let Some(draft) = &mut self.scene.draft {
            draft.style.color = self.preferences.shape_color;
            draft.style.width = self.preferences.stroke_width as f64;
        }
        self.scene.arrow_size = self.preferences.arrow_size as f64;
    }
    fn release(&mut self) {
        self.devices.mouse(LEFT_UP, self.now);
        self.halo_state.mouse(LEFT_UP, self.now);
        self.scene.finish(self.now, self.preferences.duration_ms);
    }
    /// Shortcut, travel to the start, then a drag that draws the shape as the hand moves.
    fn draw(
        &mut self,
        (letter, label, tool): (u16, char, model::LiveTool),
        from: Point,
        to: Point,
    ) -> Result<()> {
        self.shortcut(letter, label, tool)?;
        self.glide(from, 10)?;
        self.press();
        self.hold(3)?;
        self.glide(to, 22)?;
        self.hold(2)?;
        self.release();
        self.hold(8)
    }
    fn cycle(&mut self) -> Result<()> {
        let p = |x, y| Point { x, y };
        self.draw(
            (0x13, 'R', model::LiveTool::Rectangle),
            p(566., 138.),
            p(848., 316.),
        )?;
        self.draw(
            // US position of A: the site film speaks English.
            (0x1e, 'A', model::LiveTool::Arrow),
            p(380., 126.),
            p(556., 196.),
        )?;
        self.draw(
            (0x2e, 'C', model::LiveTool::Ellipse),
            p(470., 250.),
            p(519., 250.),
        )
    }
}

/// Site preview film: Alt+R, Alt+A and Alt+C repeat while the preceding circle fades.
pub(crate) fn film_over(backdrop: &crate::Image, folder: &std::path::Path) -> Result<()> {
    let fail = |e: std::io::Error| windows::core::Error::new(E_FAIL, e.to_string());
    std::fs::create_dir_all(folder).map_err(fail)?;
    let (width, height) = (backdrop.width as i32, backdrop.height as i32);
    let null = std::ptr::null();
    let lock = Layer::new(null, -32000, -32000, diagram::LOCK_SIZE, diagram::LOCK_SIZE)?;
    lock.begin(true)?;
    lock.lock_icon(false);
    lock.end()?;
    let mut film = LiveFilm {
        backdrop,
        folder: folder.to_path_buf(),
        preferences: PresentationSettings::default(),
        canvas: Layer::new(null, -32000, -32000, width, height)?,
        halo: Layer::new(null, -32000, -32000, HALO_SIZE, HALO_SIZE)?,
        keys: Layer::new(null, -32000, -32000, diagram::WIDTH, diagram::HEIGHT)?,
        lock,
        scene: model::Scene::default(),
        devices: Devices::default(),
        halo_state: halo::HaloState::default(),
        now: 0,
        pointer: Point { x: 470., y: 250. },
        pointers: Vec::new(),
        recording: false,
    };
    // Warm the scene with one cycle, then record exactly the next cycle. Its last frame flows
    // into its first: the April circle continues fading while the next rectangle shortcut starts.
    film.cycle()?;
    film.recording = true;
    film.cycle()?;
    let pointers: String = film
        .pointers
        .iter()
        .map(|at| format!("{:.1} {:.1}\n", at.x, at.y))
        .collect();
    std::fs::write(folder.join("pointer.txt"), pointers).map_err(fail)
}
