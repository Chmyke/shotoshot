//! ROLE: Give explicitly selected drawing shortcuts priority when RegisterHotKey is occupied.
//! OWNER: Bindings owns a dedicated, message-driven hook thread; no input log or polling.
use super::priority_match::{Decision, Matcher};
use super::*;
use crate::settings::Shortcut;
use std::cell::Cell;
use windows::Win32::System::Threading::GetCurrentThreadId;

struct State {
    owner: HWND,
    matcher: Matcher,
    observe: bool,
}
impl State {
    fn event(&mut self, key: u32, mods: u32, down: bool) -> bool {
        match self.matcher.event(key, mods, down) {
            Decision::Pass => false,
            Decision::Consume => true,
            Decision::Trigger(id) => {
                let posted = unsafe {
                    PostMessageW(Some(self.owner), WM_HOTKEY, WPARAM(id as usize), LPARAM(0))
                        .is_ok()
                };
                if !posted {
                    let _ = self.matcher.event(key, mods, false);
                }
                posted
            }
        }
    }
}
thread_local! {
    // PITFALL: Win32 supplies no user pointer. This thread-local dispatcher points only at
    // the hook thread's owned state, and is cleared before unhooking or releasing that state.
    static ACTIVE: Cell<*const RefCell<State>> = const { Cell::new(std::ptr::null()) };
}
pub(super) struct Priority {
    thread: Option<std::thread::JoinHandle<()>>,
    id: u32,
}
impl Priority {
    pub fn new(owner: HWND, entries: [Option<(Shortcut, i32)>; 3], observe: bool) -> Result<Self> {
        let owner = owner.0 as usize;
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("Shotoshot shortcuts".into())
            .spawn(move || {
                let state = Box::new(RefCell::new(State {
                    owner: HWND(owner as *mut _),
                    matcher: Matcher::new(entries),
                    observe,
                }));
                let result = unsafe {
                    let mut msg = MSG::default();
                    let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
                    SetWindowsHookExW(
                        WH_KEYBOARD_LL,
                        Some(callback),
                        GetModuleHandleW(None).ok().map(Into::into),
                        0,
                    )
                };
                match result {
                    Ok(hook) => {
                        ACTIVE.with(|active| active.set(&*state));
                        let id = unsafe { GetCurrentThreadId() };
                        if ready_tx.send(Ok(id)).is_ok() {
                            unsafe {
                                let mut msg = MSG::default();
                                while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                                    let _ = TranslateMessage(&msg);
                                    DispatchMessageW(&msg);
                                }
                            }
                        }
                        ACTIVE.with(|active| active.set(std::ptr::null()));
                        unsafe {
                            let _ = UnhookWindowsHookEx(hook);
                        }
                    }
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                    }
                }
            })
            .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
        match ready_rx
            .recv()
            .map_err(|e| Error::new(E_FAIL, e.to_string()))?
        {
            Ok(id) => Ok(Self {
                thread: Some(thread),
                id,
            }),
            Err(error) => {
                let _ = thread.join();
                Err(error)
            }
        }
    }
}
impl Drop for Priority {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
unsafe extern "system" fn callback(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    let consumed = std::panic::catch_unwind(|| {
        if code != HC_ACTION as i32 || l.0 == 0 {
            return false;
        }
        let down = match w.0 as u32 {
            WM_KEYDOWN | WM_SYSKEYDOWN => true,
            WM_KEYUP | WM_SYSKEYUP => false,
            _ => return false,
        };
        // PITFALL: The packet belongs to this callback only; never store it or translate text.
        let key = unsafe { &*(l.0 as *const KBDLLHOOKSTRUCT) };
        if key.flags.contains(LLKHF_INJECTED) {
            return false;
        }
        ACTIVE.with(|active| {
            let pointer = active.get();
            if pointer.is_null() {
                return false;
            }
            let Ok(mut state) = (unsafe { &*pointer }).try_borrow_mut() else {
                return false;
            };
            if state.observe {
                let flags = u16::from(!down) | (u16::from(key.flags.contains(LLKHF_EXTENDED)) << 1);
                let packet = (key.scanCode & 0xffff) | (u32::from(flags) << 16);
                unsafe {
                    let _ = PostMessageW(
                        Some(state.owner),
                        OBSERVED_KEY,
                        WPARAM(key.vkCode as usize),
                        LPARAM(packet as isize),
                    );
                }
            }
            // Only modifiers are read; the packet's main key has not updated async state yet.
            let held = held_modifiers();
            let mods = (u32::from(held[0]) * MOD_CONTROL.0)
                | (u32::from(held[1]) * MOD_ALT.0)
                | (u32::from(held[2]) * MOD_SHIFT.0)
                | (u32::from(held[3]) * MOD_WIN.0);
            state.event(key.vkCode, mods, down)
        })
    })
    .unwrap_or(false);
    if consumed {
        LRESULT(1)
    } else {
        unsafe { CallNextHookEx(None, code, w, l) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn priority_posts_one_native_command_and_preserves_plain_typing() {
        let owner = crate::resources::OwnedWindow(unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                w!("STATIC"),
                w!("Priority test"),
                WS_POPUP,
                -32000,
                -32000,
                1,
                1,
                None,
                None,
                None,
                None,
            )
            .unwrap()
        });
        let mut state = State {
            owner: owner.0,
            observe: false,
            matcher: Matcher::new([
                Some((
                    Shortcut {
                        modifiers: 1,
                        key: 82,
                    },
                    HOTKEY_BASE + 2,
                )),
                None,
                None,
            ]),
        };
        assert!(!state.event(82, 0, true));
        assert!(!state.event(82, 0, false));
        assert!(state.event(82, 1, true));
        assert!(state.event(82, 1, true));
        assert!(!state.event(18, 0, false));
        assert!(state.event(82, 0, false));
        unsafe {
            let mut message = MSG::default();
            assert!(
                PeekMessageW(&mut message, Some(owner.0), WM_HOTKEY, WM_HOTKEY, PM_REMOVE)
                    .as_bool()
            );
            assert_eq!(message.wParam.0, (HOTKEY_BASE + 2) as usize);
            assert!(
                !PeekMessageW(&mut message, Some(owner.0), WM_HOTKEY, WM_HOTKEY, PM_REMOVE)
                    .as_bool()
            );
        }
        assert!(!state.event(82, 0, true));
    }
}
