//! ROLE: Reopens a `.shoto` document as a capture session on today's screens.
//! INVARIANTS: Nothing is decoded before the frame is known to fit a screen, and only as
//! the PNG of the announced size.
use super::*;
use shotoshot_core::shoto::Scene;

impl Session {
    /// A session on the document `scene`, whose frame is the PNG `frame`, reopened from
    /// `path`: Save rewrites that file.
    pub fn reopen(scene: Scene, frame: &[u8], path: &std::path::Path) -> Result<Self> {
        let damaged = || crate::failure::visible(shoto::Error::Damaged.key(), &[]);
        let size = Point {
            x: scene.width as f64,
            y: scene.height as f64,
        };
        let origin = Point {
            x: scene.origin.0 as f64,
            y: scene.origin.1 as f64,
        };
        let screens: Vec<Rect> = hint::monitors_from(0, 0).iter().map(|m| m.bounds).collect();
        let mut pointer = POINT::default();
        // Without a pointer position, the first screen stands in for the pointer's.
        let _ = unsafe { GetCursorPos(&mut pointer) };
        let pointer = Point {
            x: pointer.x as f64,
            y: pointer.y as f64,
        };
        let corner = shotoshot_ui::placement(size, origin, &screens, pointer).ok_or_else(|| {
            crate::failure::visible(
                shotoshot_text::Key::ErrorDocumentTooLarge,
                &[
                    ("width", &scene.width.to_string()),
                    ("height", &scene.height.to_string()),
                ],
            )
        })?;
        let mut frame = decode_png(frame, (scene.width, scene.height)).map_err(|_| damaged())?;
        // A capture is opaque; the file is not trusted to say so.
        for pixel in frame.pixels.as_chunks_mut::<4>().0 {
            pixel[3] = 255;
        }
        (frame.left, frame.top) = (corner.x as i32, corner.y as i32);
        let mut renderer = Renderer::with(Backend::Hardware, false)?;
        renderer.set_image(&frame)?;
        let monitors = hint::monitors_from(frame.left, frame.top);
        let measure =
            |text: &str, size: f64| renderer.measure_annotation_text(text, size as f32).ok();
        let capture = Capture::reopen(scene, frame, monitors, measure).ok_or_else(damaged)?;
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
            document: Some(path.into()),
            board: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: f64, y: f64) -> Point {
        Point { x, y }
    }
    /// A small capture with a rectangle, an arrow and a text over a patterned frame.
    fn annotated() -> Session {
        let mut image = Image::solid(320, 200, [0, 0, 0, 255]);
        for (i, pixel) in image.pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            (pixel[0], pixel[1], pixel[2]) = ((i % 320) as u8, (i / 320) as u8, 90);
        }
        let mut session = Session::new(image).unwrap();
        let capture = &mut session.capture;
        capture.region = Rect::new(point(40., 30.), point(280., 170.));
        capture.selecting = false;
        for (tool, from, to) in [
            (Tool::Rectangle, point(60., 50.), point(140., 110.)),
            (Tool::Arrow, point(150., 60.), point(250., 150.)),
        ] {
            capture.editor.tool = tool;
            capture.editor.begin(from, Modifiers::default());
            capture.editor.update(to);
            capture.editor.finish();
        }
        let extent = session
            .native
            .renderer
            .measure_annotation_text("Note", 24.)
            .unwrap();
        session
            .capture
            .editor
            .add_text_measured(point(70., 120.), "Note".into(), extent);
        session
    }
    fn folder(name: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!("shotoshot-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn a_saved_document_shows_as_a_png_and_reopens_as_the_capture() {
        let _com = crate::ComApartment::new().unwrap();
        let session = annotated();
        let folder = folder("document");
        let path = folder.join("Shot 001.shoto");
        let bytes = session.document_bytes(&session.capture.image).unwrap();
        save_bytes(&bytes, &path, false).unwrap();
        // Any PNG reader shows the annotated region, nothing more.
        let visible = load_png(&path).unwrap();
        let flattened = session.export().unwrap();
        assert_eq!((visible.width, visible.height), (240, 140));
        assert_eq!(visible.pixels, flattened.pixels);
        // Shotoshot finds the capture again: frame, region and objects.
        let file = std::fs::read(&path).unwrap();
        let (scene, frame) = shoto::unpack(&file).unwrap();
        assert_eq!((scene.width, scene.height), (320, 200));
        let reopened = Session::reopen(scene, frame, &path).unwrap();
        assert_eq!(reopened.capture.image.pixels, session.capture.image.pixels);
        assert_eq!(reopened.capture.region, session.capture.region);
        assert_eq!(
            reopened.capture.editor.objects(),
            session.capture.editor.objects()
        );
        assert!(reopened.capture.reopened);
        assert_eq!(reopened.document.as_deref(), Some(path.as_path()));
        assert_eq!(reopened.export().unwrap().pixels, flattened.pixels);
        std::fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn saving_a_reopened_document_rewrites_its_file_and_nothing_else() {
        let _com = crate::ComApartment::new().unwrap();
        let session = annotated();
        let folder = folder("rewrite");
        let path = folder.join("report.shoto");
        let bytes = session.document_bytes(&session.capture.image).unwrap();
        save_bytes(&bytes, &path, false).unwrap();
        let (scene, frame) = shoto::unpack(&bytes).unwrap();
        let mut reopened = Session::reopen(scene, frame, &path).unwrap();
        let moved = reopened.capture.editor.objects()[0].id;
        reopened.capture.editor.select_only(moved);
        reopened.capture.editor.nudge(point(7., 0.));
        let mut settings = Settings::default();
        let before = settings.folder.clone();
        let saved = reopened.save(HWND::default(), &mut settings, false);
        assert_eq!(saved.unwrap(), Saved::Document);
        assert_eq!(settings.folder, before);
        let names: Vec<_> = std::fs::read_dir(&folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["report.shoto"]);
        let file = std::fs::read(&path).unwrap();
        let (scene, frame) = shoto::unpack(&file).unwrap();
        let again = Session::reopen(scene, frame, &path).unwrap();
        assert_eq!(
            again.capture.editor.objects(),
            reopened.capture.editor.objects()
        );
        assert_ne!(
            again.capture.editor.objects(),
            session.capture.editor.objects()
        );
        assert_eq!(again.capture.image.pixels, session.capture.image.pixels);
        std::fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn a_frame_that_is_not_the_announced_png_is_refused() {
        let _com = crate::ComApartment::new().unwrap();
        let session = annotated();
        let path = std::path::Path::new("unused.shoto");
        let bytes = session.document_bytes(&session.capture.image).unwrap();
        let (scene, frame) = shoto::unpack(&bytes).unwrap();
        let mut wider = scene.clone();
        wider.width += 1;
        assert!(Session::reopen(wider, frame, path).is_err());
        assert!(Session::reopen(scene.clone(), b"GIF89a", path).is_err());
        let mut huge = scene;
        (huge.width, huge.height) = (32_000, 32_000);
        let error = Session::reopen(huge, frame, path).err().unwrap();
        let text = crate::failure::text_in(&error, shotoshot_text::Language::En);
        assert!(
            text.as_str()
                .contains("32000 × 32000) is larger than this screen")
        );
    }
}
