//! ROLE: Applies the presentation commands of the notification menu (built by App).
use super::*;
use crate::settings::INPUT_SCALES;

/// Shape lifetime chosen by a duration command of the notification menu.
pub(crate) fn duration_ms(command: u32) -> Option<u32> {
    Some(match command {
        121 => 1000,
        122 => 3000,
        123 => 5000,
        _ => return None,
    })
}
/// Size of the input diagram, in percent, chosen by a size command of the notification menu.
pub(crate) fn scale(command: u32) -> Option<u32> {
    INPUT_SCALES
        .get(command.checked_sub(141)? as usize)
        .copied()
}
/// The preferences a choice of a submenu leaves, None for any other command.
pub(crate) fn preference(
    command: u32,
    current: &PresentationSettings,
) -> Option<PresentationSettings> {
    let mut next = current.clone();
    if let Some(duration) = duration_ms(command) {
        next.duration_ms = duration;
    } else {
        next.input_scale = scale(command)?;
    }
    Some(next)
}
impl Presentation {
    pub fn command(&mut self, command: u32) -> Result<()> {
        let mut next = self.enabled;
        match command {
            101 => next.halo = !next.halo,
            107 => next.loupe = !next.loupe,
            102 => next.shapes = !next.shapes,
            103 => next.keys = !next.keys,
            104 => next.mouse = !next.mouse,
            106 => return self.start_move(),
            _ => return Ok(()),
        }
        self.set_enabled(next)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_submenu_choice_changes_exactly_one_preference() {
        let current = PresentationSettings::default();
        let expected = |change: fn(&mut PresentationSettings)| {
            let mut next = current.clone();
            change(&mut next);
            Some(next)
        };
        assert_eq!(
            preference(123, &current),
            expected(|next| next.duration_ms = 5000)
        );
        assert_eq!(
            preference(142, &current),
            expected(|next| next.input_scale = 150)
        );
        // Switches, shortcut rows, submenu rows and ids past each list are not choices.
        for command in [101, 103, 104, 106, 107, 110, 120, 124, 131, 133, 140, 144] {
            assert_eq!(preference(command, &current), None, "{command}");
        }
    }
}
