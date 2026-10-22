//! Windows glue for the portable toolbar.
pub use shotoshot_ui::toolbar::*;
/// Win32 timer id driving hover-hint delays and fades, and the toolbar's one-shot motion.
pub const HINT_TIMER: usize = 0x5348;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::Renderer;
    use shotoshot_text::Language;

    /// Measures each hint with DirectWrite as `draw_hint` draws it; no screen is read.
    #[test]
    fn every_tooltip_fits() {
        let _com = crate::ComApartment::new().unwrap();
        let renderer = Renderer::software().unwrap();
        for language in Language::ALL {
            for hint in all_tooltips_in(language) {
                let width = renderer.measure_text_in(&hint, 12., language).unwrap().x;
                assert!(
                    width <= MAX_HINT_WIDTH,
                    "{}: `{hint}` is {width:.0} px wide, over {MAX_HINT_WIDTH} px",
                    language.tag()
                );
            }
        }
    }
}
