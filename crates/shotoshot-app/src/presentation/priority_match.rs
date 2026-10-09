//! ROLE: Matches priority presentation shortcuts without altering other input.
//! OWNER: The low-level keyboard hook owns one matcher for its lifetime.
//! INVARIANTS: Exact modifiers, one trigger per physical press, balanced consumed releases.
use crate::settings::Shortcut;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Decision {
    Pass,
    Consume,
    Trigger(i32),
}
pub(super) struct Matcher {
    entries: [Option<(Shortcut, i32)>; 3],
    intercepted: [bool; 256],
}
impl Matcher {
    pub fn new(entries: [Option<(Shortcut, i32)>; 3]) -> Self {
        Self {
            entries,
            intercepted: [false; 256],
        }
    }
    pub fn event(&mut self, key: u32, modifiers: u32, down: bool) -> Decision {
        let Some(intercepted) = self.intercepted.get_mut(key as usize) else {
            return Decision::Pass;
        };
        if *intercepted {
            if !down {
                *intercepted = false;
            }
            return Decision::Consume;
        }
        if down {
            for &(shortcut, id) in self.entries.iter().flatten() {
                if shortcut.key == key && shortcut.modifiers == modifiers {
                    *intercepted = true;
                    return Decision::Trigger(id);
                }
            }
        }
        Decision::Pass
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn binding(modifiers: u32, key: u32, id: i32) -> Option<(Shortcut, i32)> {
        Some((Shortcut { modifiers, key }, id))
    }
    #[test]
    fn exact_modifiers_and_other_keys_pass() {
        let mut m = Matcher::new([binding(1, 82, 7), None, None]);
        for modifiers in [0, 2, 3, 5, 9, 15] {
            assert_eq!(m.event(82, modifiers, true), Decision::Pass);
            assert_eq!(m.event(82, modifiers, false), Decision::Pass);
        }
        for key in [16, 17, 18, 65, 91, 255, 256, u32::MAX] {
            assert_eq!(m.event(key, 1, true), Decision::Pass);
            assert_eq!(m.event(key, 1, false), Decision::Pass);
        }
        assert_eq!(m.event(82, 1, true), Decision::Trigger(7));
    }
    #[test]
    fn repeats_and_release_after_alt_are_consumed_once() {
        let mut m = Matcher::new([binding(1, 82, 7), None, None]);
        assert_eq!(m.event(82, 1, true), Decision::Trigger(7));
        assert_eq!(m.event(82, 1, true), Decision::Consume);
        assert_eq!(m.event(18, 0, false), Decision::Pass);
        assert_eq!(m.event(82, 0, true), Decision::Consume);
        assert_eq!(m.event(82, 0, false), Decision::Consume);
        assert_eq!(m.event(82, 0, false), Decision::Pass);
        assert_eq!(m.event(82, 1, true), Decision::Trigger(7));
        assert_eq!(m.event(82, 1, false), Decision::Consume);
    }
    #[test]
    fn same_key_different_modifiers_and_concurrent_keys() {
        let mut m = Matcher::new([binding(1, 82, 7), binding(2, 82, 8), binding(1, 67, 9)]);
        assert_eq!(m.event(82, 1, true), Decision::Trigger(7));
        assert_eq!(m.event(67, 1, true), Decision::Trigger(9));
        assert_eq!(m.event(82, 2, true), Decision::Consume);
        assert_eq!(m.event(82, 0, false), Decision::Consume);
        assert_eq!(m.event(82, 2, true), Decision::Trigger(8));
        assert_eq!(m.event(67, 0, false), Decision::Consume);
        assert_eq!(m.event(82, 0, false), Decision::Consume);
    }
}
