use crate::*;
use shotoshot_core::*;
use windows::core::Result;
mod shape_samples;

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
    let mut guidance = crate::session::Session::new(Image::solid(640, 360, [80, 85, 95, 255]))?;
    guidance.capture.pointer = Point { x: 270., y: 130. };
    let preview = paint_image(640, 360, |hdc| guidance.paint(hdc))?;
    save_png(
        &preview,
        &path.with_file_name("shotoshot-selection-hint.png"),
    )?;
    shape_samples::render(&path.with_file_name("shotoshot-shape-scales.png"))?;
    crate::presentation::demo::render()
}
