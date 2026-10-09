//! ROLE: Tracks pointer-button state and the short click pulse from native mouse events.
//! OWNER: One presentation controller owns one HaloState.
//! INVARIANTS: Release clears pressed immediately; the pulse never prolongs pressed colour.

const PULSE_MS: u64 = 300;

#[derive(Default)]
pub(super) struct HaloState {
    buttons: [bool; 3],
    pulse_start: Option<u64>,
}

impl HaloState {
    pub fn mouse(&mut self, flags: u16, now: u64) {
        for index in 0..self.buttons.len() {
            if flags & (1 << (index * 2)) != 0 {
                self.buttons[index] = true;
                self.pulse_start = Some(now);
            }
            if flags & (2 << (index * 2)) != 0 {
                self.buttons[index] = false;
            }
        }
    }

    pub fn pressed(&self) -> bool {
        self.buttons.iter().any(|down| *down)
    }

    /// Progress from zero to one over 300 ms. Draw only while needs_animation is true.
    pub fn pulse(&self, now: u64) -> f32 {
        self.pulse_start.map_or(0., |start| {
            now.saturating_sub(start).min(PULSE_MS) as f32 / PULSE_MS as f32
        })
    }

    pub fn needs_animation(&self, now: u64) -> bool {
        self.pulse_start
            .is_some_and(|start| now.saturating_sub(start) < PULSE_MS)
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_click_releases_immediately_while_pulse_continues() {
        let mut halo = HaloState::default();
        assert!(!halo.pressed());
        assert!(!halo.needs_animation(0));
        halo.mouse(1, 100);
        assert!(halo.pressed());
        assert_eq!(halo.pulse(100), 0.);
        halo.mouse(2, 101);
        assert!(!halo.pressed());
        assert!(halo.needs_animation(101));
        halo.mouse(1 | 2, 110);
        assert!(
            !halo.pressed(),
            "a packet containing down then up is released"
        );
        assert_eq!(halo.pulse(110), 0.);
    }

    #[test]
    fn releasing_one_button_preserves_other_held_buttons() {
        let mut halo = HaloState::default();
        halo.mouse(1 | 4 | 16, 10);
        halo.mouse(2, 20);
        assert!(halo.pressed());
        halo.mouse(8, 30);
        assert!(halo.pressed());
        halo.mouse(32, 40);
        assert!(!halo.pressed());
        halo.mouse(2 | 8 | 32 | 0xc00, 50);
        assert!(!halo.pressed());
        assert_eq!(
            halo.pulse(160),
            0.5,
            "wheel and releases do not restart the pulse"
        );
    }

    #[test]
    fn pulse_expires_while_held_and_restarts_on_the_next_press() {
        let mut halo = HaloState::default();
        halo.mouse(1, 100);
        assert_eq!(halo.pulse(250), 0.5);
        assert!(halo.needs_animation(399));
        assert!(!halo.needs_animation(400));
        assert_eq!(halo.pulse(500), 1.);
        assert!(
            halo.pressed(),
            "a held button does not depend on the animation timer"
        );
        halo.mouse(4, 600);
        assert_eq!(halo.pulse(600), 0.);
        assert!(halo.needs_animation(600));
        halo.clear();
        assert!(!halo.pressed());
        assert!(!halo.needs_animation(600));
        assert_eq!(halo.pulse(600), 0.);
    }
}
