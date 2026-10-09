//! Editor lifecycle and display changes can arrive inside a modal loop while App is borrowed. Retain them
//! outside that borrow and retry only while there is deferred work.
use super::*;
use std::collections::VecDeque;
type Event = (HWND, u32, WPARAM, LPARAM);
pub(super) const RETRY: usize = 0x5345;
thread_local! {
    static PENDING: RefCell<VecDeque<Event>> = const { RefCell::new(VecDeque::new()) };
}
pub(super) fn defer(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) {
    PENDING.with_borrow_mut(|events| {
        // Only coalesce adjacent topology notifications; preserve ordering around editor events.
        if msg == WM_DISPLAYCHANGE
            && let Some(last) = events.back_mut()
            && last.0 == hwnd
            && last.1 == msg
        {
            *last = (hwnd, msg, w, l);
        } else {
            events.push_back((hwnd, msg, w, l));
        }
    });
    unsafe {
        SetTimer(Some(hwnd), RETRY, 30, None);
    }
}
pub(super) fn take(hwnd: HWND) -> Vec<Event> {
    unsafe {
        let _ = KillTimer(Some(hwnd), RETRY);
    }
    PENDING.with_borrow_mut(|events| {
        let mut ready = Vec::new();
        events.retain(|event| {
            if event.0 == hwnd {
                ready.push(*event);
                false
            } else {
                true
            }
        });
        ready
    })
}

pub(super) fn dispatch(
    hwnd: HWND,
    mut handle: impl FnMut(u32, WPARAM, LPARAM) -> Result<()>,
) -> Result<()> {
    let mut result = Ok(());
    for (_, message, w, l) in take(hwnd) {
        // A failed reconstruction must not discard READY/EXIT from the same modal-loop batch.
        let current = handle(message, w, l);
        if result.is_ok() {
            result = current;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_display_rebuild_does_not_lose_editor_notifications() {
        let window = HWND(21 as _);
        defer(window, WM_DISPLAYCHANGE, WPARAM(32), LPARAM(0));
        defer(window, editor_process::READY, WPARAM(9), LPARAM(1));
        defer(window, editor_process::EXITED, WPARAM(9), LPARAM(0));
        let mut received = Vec::new();
        let result = dispatch(window, |message, _, _| {
            received.push(message);
            if message == WM_DISPLAYCHANGE {
                Err(Error::new(E_FAIL, "surface unavailable"))
            } else {
                Ok(())
            }
        });
        assert!(result.is_err());
        assert_eq!(
            received,
            [
                WM_DISPLAYCHANGE,
                editor_process::READY,
                editor_process::EXITED
            ]
        );
    }
    #[test]
    fn display_bursts_coalesce_without_crossing_editor_lifecycle_events() {
        let window = HWND(19 as _);
        defer(window, WM_DISPLAYCHANGE, WPARAM(32), LPARAM(1));
        defer(window, WM_DISPLAYCHANGE, WPARAM(32), LPARAM(2));
        defer(window, editor_process::EXITED, WPARAM(9), LPARAM(0));
        defer(window, WM_DISPLAYCHANGE, WPARAM(32), LPARAM(3));
        let events = take(window);
        assert_eq!(events.len(), 3);
        assert_eq!((events[0].1, events[0].3.0), (WM_DISPLAYCHANGE, 2));
        assert_eq!(events[1].1, editor_process::EXITED);
        assert_eq!((events[2].1, events[2].3.0), (WM_DISPLAYCHANGE, 3));
    }
    #[test]
    fn deferred_events_survive_a_borrow_and_keep_their_order() {
        let window = HWND(17 as _);
        defer(window, editor_process::READY, WPARAM(9), LPARAM(10));
        defer(window, editor_process::EXITED, WPARAM(9), LPARAM(0));
        let events = take(window);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].1, editor_process::READY);
        assert_eq!(events[1].1, editor_process::EXITED);
        assert!(take(window).is_empty());
    }
}
