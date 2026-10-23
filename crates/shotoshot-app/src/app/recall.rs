//! ROLE: The previous capture's region, offered again to a capture that follows soon after.
//! INVARIANTS: Kept in the resident's memory only, never on disk: a restart forgets it, as
//! does any capture ended without a region. Travels between processes in one `LPARAM`, four
//! signed 16-bit screen coordinates; an empty rectangle means none.
use std::time::{Duration, Instant};
use windows::Win32::{
    Foundation::{LPARAM, RECT},
    UI::WindowsAndMessaging::WM_APP,
};

/// Editor to resident, at the end of a capture: `WPARAM` its process id, `LPARAM` its region.
pub(super) const REGION: u32 = WM_APP + 15;
/// How long a region is offered again (Mickaël, 2026-10-23): a series of captures keeps it,
/// one made after a pause starts blank.
const KEPT: Duration = Duration::from_secs(5 * 60);

pub(super) fn pack(region: Option<RECT>) -> LPARAM {
    let Some(r) = region.filter(|r| fits(r) && r.right > r.left && r.bottom > r.top) else {
        return LPARAM(0);
    };
    let word = |v: i32| v as i16 as u16 as u64;
    LPARAM((word(r.left) | word(r.top) << 16 | word(r.right) << 32 | word(r.bottom) << 48) as isize)
}
pub(super) fn unpack(l: LPARAM) -> Option<RECT> {
    let at = |shift: u32| (l.0 as u64 >> shift) as u16 as i16 as i32;
    let r = RECT {
        left: at(0),
        top: at(16),
        right: at(32),
        bottom: at(48),
    };
    (r.right > r.left && r.bottom > r.top).then_some(r)
}
fn fits(r: &RECT) -> bool {
    [r.left, r.top, r.right, r.bottom]
        .iter()
        .all(|&v| i16::try_from(v).is_ok())
}

/// What one process knows: the resident, the last region and when it came; the editor, the
/// region its capture was offered.
#[derive(Default)]
pub(super) struct Recall {
    kept: Option<(RECT, Instant)>,
    offered: Option<RECT>,
}
impl Recall {
    /// The resident hears how a capture ended: its region, or none to forget.
    pub fn keep(&mut self, region: Option<RECT>, now: Instant) {
        self.kept = region.map(|r| (r, now));
    }
    /// The region to offer a capture starting `now`, if the last one is recent enough.
    pub fn recent(&self, now: Instant) -> Option<RECT> {
        self.kept
            .filter(|(_, at)| now.saturating_duration_since(*at) < KEPT)
            .map(|(r, _)| r)
    }
    /// The editor receives the region offered with its capture request.
    pub fn offer(&mut self, region: Option<RECT>) {
        self.offered = region;
    }
    pub fn take_offer(&mut self) -> Option<RECT> {
        self.offered.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regions_cross_processes_in_one_lparam_negative_coordinates_included() {
        let r = RECT {
            left: -1920,
            top: -200,
            right: 640,
            bottom: 1080,
        };
        let back = unpack(pack(Some(r))).unwrap();
        assert_eq!(
            (back.left, back.top, back.right, back.bottom),
            (-1920, -200, 640, 1080)
        );
        assert_eq!(unpack(pack(None)), None);
        let huge = RECT {
            right: 40_000,
            bottom: 10,
            ..r
        };
        assert_eq!(
            unpack(pack(Some(huge))),
            None,
            "too far to travel: forgotten"
        );
    }
    #[test]
    fn a_region_is_offered_for_five_minutes_and_forgotten_on_an_empty_end() {
        let r = RECT {
            left: 10,
            top: 20,
            right: 300,
            bottom: 200,
        };
        let start = Instant::now();
        let mut recall = Recall::default();
        assert_eq!(recall.recent(start), None);
        recall.keep(Some(r), start);
        assert_eq!(recall.recent(start + Duration::from_secs(299)), Some(r));
        assert_eq!(recall.recent(start + Duration::from_secs(300)), None);
        recall.keep(None, start);
        assert_eq!(recall.recent(start), None);
    }
}
