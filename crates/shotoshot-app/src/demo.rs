use crate::*;
use shotoshot_core::*;
use windows::core::Result;
mod shape_samples;

/// The capture as a slate: a dark background, chalk drawing and two filled shapes.
fn slate() -> Editor {
    const CHALK: [u8; 3] = [245, 245, 248];
    let at = |x: f64, y: f64| Point { x, y };
    let mut editor = Editor::default();
    editor.style.color = [28, 31, 40];
    editor.fill_at(at(100., 120.), false);
    editor.style = Style {
        color: CHALK,
        width: 4.,
    };
    let curve: Vec<_> = (0..=42)
        .map(|i| {
            let x = i as f64 * 10.;
            at(160. + x, 520. - x * x / 560.)
        })
        .collect();
    for (tool, points) in [
        (Tool::Arrow, vec![at(160., 520.), at(160., 170.)]),
        (Tool::Arrow, vec![at(160., 520.), at(640., 520.)]),
        (Tool::Pen, curve),
        (Tool::Rectangle, vec![at(720., 170.), at(960., 310.)]),
        (Tool::Ellipse, vec![at(840., 450.), at(930., 450.)]),
    ] {
        editor.tool = tool;
        editor.begin(points[0], Modifiers::default());
        for point in &points[1..] {
            editor.update(*point);
        }
        editor.finish();
    }
    editor.style.color = [80, 153, 244];
    editor.fill_at(at(840., 240.), false);
    editor.style.color = [250, 192, 66];
    editor.fill_at(at(840., 450.), false);
    editor.style.color = CHALK;
    editor.add_text(at(200., 180.), "y = x\u{b2}".into());
    editor.add_text(at(755., 222.), "Filled shape".into());
    editor.tool = Tool::Pen;
    editor
}

pub fn render_demo() -> Result<()> {
    let mut image = Image::solid(1100, 680, [36, 29, 24, 255]);
    for y in 100..600 {
        for x in 70..1030 {
            let index = (y * 1100 + x) * 4;
            image.pixels[index..index + 4].copy_from_slice(&[248, 245, 242, 255]);
        }
    }
    let mut editor = Editor::default();
    editor.tool = Tool::Rectangle;
    editor.begin(Point { x: 130., y: 200. }, Modifiers::default());
    editor.update(Point { x: 410., y: 400. });
    editor.finish();
    editor.style.color = [80, 153, 244];
    editor.tool = Tool::Ellipse;
    editor.begin(Point { x: 800., y: 300. }, Modifiers::default());
    editor.update(Point { x: 900., y: 300. });
    editor.finish();
    editor.tool = Tool::Arrow;
    editor.style.color = [40, 60, 80];
    editor.begin(
        Point { x: 400., y: 300. },
        Modifiers {
            ctrl: true,
            ..Modifiers::default()
        },
    );
    editor.update(Point { x: 720., y: 300. });
    editor.finish();
    editor.add_text(Point { x: 150., y: 240. }, "Editable objects".into());
    editor.add_text(Point { x: 730., y: 280. }, "Connected".into());
    editor.add_text(
        Point { x: 130., y: 480. },
        "Shotoshot  ·  Native Windows capture + annotations".into(),
    );
    let renderer = Renderer::new()?;
    let output = renderer.export(
        &image,
        &[],
        editor.objects(),
        Rect {
            min: Point::default(),
            max: Point { x: 1100., y: 680. },
        },
    )?;
    let path = std::env::current_dir()
        .map_err(|e| windows::core::Error::new(windows::Win32::Foundation::E_FAIL, e.to_string()))?
        .join("target/shotoshot-demo.png");
    save_png(&output, &path)?;
    let mut session = crate::session::Session::new(image)?;
    session.capture.editor = editor;
    session.capture.region = Rect::new(Point { x: 70., y: 100. }, Point { x: 1030., y: 600. });
    session.capture.selecting = false;
    let preview = paint_image(1100, 680, |hdc| session.paint(hdc))?;
    save_png(&preview, &path.with_file_name("shotoshot-interface.png"))?;
    session.capture.picker = Some(shotoshot_ui::color_picker::ColorPicker::new(
        [80, 153, 244],
        Point { x: 755., y: 260. },
        1100,
        680,
        1.,
    ));
    let preview = paint_image(1100, 680, |hdc| session.paint(hdc))?;
    save_png(&preview, &path.with_file_name("shotoshot-color-wheel.png"))?;
    session.capture.picker = None;
    session.capture.editor.tool = Tool::Pen;
    session.capture.editor.style.width = 18.;
    session.capture.width_preview = true;
    session.capture.pointer = Point { x: 580., y: 390. };
    let preview = paint_image(1100, 680, |hdc| session.paint(hdc))?;
    save_png(
        &preview,
        &path.with_file_name("shotoshot-stroke-preview.png"),
    )?;
    session.capture.editor = slate();
    session.capture.width_preview = false;
    session.capture.bucket = true;
    // On the chalk curve, in chalk: the bucket would change nothing, so it outlines nothing.
    session.capture.pointer = Point { x: 300., y: 485. };
    let preview = paint_image(1100, 680, |hdc| session.paint(hdc))?;
    save_png(&preview, &path.with_file_name("shotoshot-slate.png"))?;
    // What a click would take: a dashed frame with Select, the zone in dashes of the bucket's colour.
    session.capture.bucket = false;
    session.capture.editor.tool = Tool::Select;
    session.capture.pointer = Point { x: 900., y: 290. };
    let preview = paint_image(1100, 680, |hdc| session.paint(hdc))?;
    save_png(&preview, &path.with_file_name("shotoshot-hover.png"))?;
    session.capture.bucket = true;
    session.capture.editor.style.color = [250, 192, 66];
    let preview = paint_image(1100, 680, |hdc| session.paint(hdc))?;
    save_png(
        &preview,
        &path.with_file_name("shotoshot-bucket-preview.png"),
    )?;
    let mut guidance = crate::session::Session::new(Image::solid(640, 360, [80, 85, 95, 255]))?;
    guidance.capture.pointer = Point { x: 270., y: 130. };
    let preview = paint_image(640, 360, |hdc| guidance.paint(hdc))?;
    save_png(
        &preview,
        &path.with_file_name("shotoshot-selection-hint.png"),
    )?;
    dense_interface(&path.with_file_name("shotoshot-interface-200.png"))?;
    shape_samples::render(&path.with_file_name("shotoshot-shape-scales.png"))?;
    crate::presentation::demo::render()
}

/// The interface on a 200 % screen, from real gestures: controls, handles and the default
/// stroke are twice their 100 % size. Nobody here owns such a screen, so this render is
/// the visual proof (`shotoshot_ui::scale_tests` proves the geometry).
fn dense_interface(path: &std::path::Path) -> Result<()> {
    use shotoshot_ui::toolbar::Action;
    let screen = Rect {
        min: Point::default(),
        max: Point { x: 2200., y: 1360. },
    };
    let mut session = crate::session::Session::new(Image::solid(2200, 1360, [248, 245, 242, 255]))?;
    session.capture.monitors = vec![shotoshot_ui::Monitor {
        bounds: screen,
        scale: 2.,
    }];
    let window = windows::Win32::Foundation::HWND::default();
    let drag = |session: &mut crate::session::Session, from: Point, to: Point| -> Result<()> {
        session.mouse_down(window, from)?;
        session.mouse_up(window, to);
        Ok(())
    };
    drag(
        &mut session,
        Point { x: 140., y: 200. },
        Point { x: 2060., y: 1200. },
    )?;
    session.action(window, Action::Tool(Tool::Arrow));
    drag(
        &mut session,
        Point { x: 1200., y: 500. },
        Point { x: 1800., y: 900. },
    )?;
    session.action(window, Action::Tool(Tool::Rectangle));
    drag(
        &mut session,
        Point { x: 400., y: 500. },
        Point { x: 1000., y: 900. },
    )?;
    session.action(window, Action::Tool(Tool::Select));
    let edge = Point { x: 400., y: 700. };
    drag(&mut session, edge, edge)?;
    session.mouse_move(window, Point { x: 1500., y: 1000. });
    let preview = paint_image(2200, 1360, |hdc| session.paint(hdc))?;
    save_png(&preview, path)
}
