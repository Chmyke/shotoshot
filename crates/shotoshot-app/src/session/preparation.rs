//! Software resources held by a hidden editor before the user asks for fresh pixels.
use super::*;

pub(crate) struct Preparation {
    renderer: Renderer,
    surface: crate::image::LiveSurface,
    size: (u32, u32),
}
impl Preparation {
    pub fn new(width: u32, height: u32, live: bool) -> Result<Self> {
        let prepared = Self::cold(width, height, live)?;
        prepared
            .renderer
            .begin(prepared.surface.dc(), width as i32, height as i32)?;
        prepared.renderer.end()?;
        Ok(prepared)
    }
    pub fn cold(width: u32, height: u32, live: bool) -> Result<Self> {
        let renderer = Renderer::with(Backend::Software, live)?;
        let surface = crate::image::LiveSurface::new(width, height)?;
        Ok(Self {
            renderer,
            surface,
            size: (width, height),
        })
    }
    pub fn activate(mut self, image: Image, live: bool) -> Result<Session> {
        image.validate()?;
        if self.renderer.transparent() != live {
            self.renderer = Renderer::with(Backend::Software, live)?;
        }
        if self.size != (image.width, image.height) {
            self.surface = crate::image::LiveSurface::new(image.width, image.height)?;
        }
        if !live {
            self.renderer.set_image(&image)?;
        }
        let mut capture = Capture::new(image);
        capture.live = live;
        let (live_surface, back_buffer) = if live {
            (Some(self.surface), None)
        } else {
            (None, Some(self.surface))
        };
        Ok(Session {
            capture,
            native: Native {
                renderer: self.renderer,
                text: None,
                dimensions: None,
                live_surface,
                live_sample: None,
                back_buffer,
                acceleration: None,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn activation_reuses_software_resources_and_paints_only_the_fresh_image() {
        let _com = ComApartment::new().unwrap();
        let prepared = Preparation::new(160, 100, false).unwrap();
        let renderer = prepared.renderer.dc.as_raw();
        let buffer = prepared.surface.dc();
        let mut image = Image::solid(160, 100, [17, 53, 91, 255]);
        image.left = -160;
        let mut session = prepared.activate(image, false).unwrap();
        assert_eq!(session.native.renderer.dc.as_raw(), renderer);
        assert_eq!(session.native.back_buffer.as_ref().unwrap().dc(), buffer);
        assert_eq!(session.capture.image.left, -160);
        session.capture.region = Rect::new(Point { x: 0., y: 0. }, Point { x: 160., y: 100. });
        session.capture.selecting = false;
        let rendered =
            crate::paint_image(160, 100, |dc| session.paint_window(HWND::default(), dc)).unwrap();
        assert!(
            rendered
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| p[..3] == [17, 53, 91])
        );
    }
    #[test]
    fn activation_honors_a_changed_mode_and_display_size() {
        let _com = ComApartment::new().unwrap();
        let prepared = Preparation::new(120, 80, false).unwrap();
        let image = Image::solid(180, 110, [0, 0, 0, 255]);
        let session = prepared.activate(image, true).unwrap();
        assert!(session.capture.live);
        assert!(session.native.renderer.transparent());
        assert!(session.native.back_buffer.is_none());
        assert_eq!(
            session.native.live_surface.as_ref().unwrap().bits().len(),
            180 * 110 * 4
        );
        assert!(session.native.live_sample.is_none());
    }
}
