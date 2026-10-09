//! ROLE: Registers opt-in native input and presentation shortcuts transactionally.
//! OWNER: Presentation; registrations are released on stop and process teardown.
//! INVARIANTS: Only selected conflicting drawing shortcuts may be consumed; no input history.
use super::*;

/// Ctrl+Alt+Esc, reserved by `PresentationSettings::validate_shape_shortcuts`.
const STOP_SHORTCUT: crate::settings::Shortcut = crate::settings::Shortcut {
    modifiers: (MOD_CONTROL.0 | MOD_ALT.0),
    key: VK_ESCAPE.0 as u32,
};

pub(super) struct Bindings {
    owner: HWND,
    enabled: Enabled,
    pub shortcuts: [crate::settings::Shortcut; 3],
    /// Turns everything off. Only the regression probe replaces it, to stay clear of a running resident.
    pub stop: crate::settings::Shortcut,
    pub conflicts: [bool; 4],
    priority: Option<super::priority::Priority>,
}
impl Bindings {
    pub fn new(owner: HWND) -> Self {
        Self {
            owner,
            enabled: Enabled::default(),
            shortcuts: PresentationSettings::default().shape_shortcuts,
            stop: STOP_SHORTCUT,
            conflicts: [false; 4],
            priority: None,
        }
    }
    pub fn set(&mut self, enabled: Enabled) -> Result<()> {
        if !enabled.shapes {
            self.priority = None;
        }
        let before = Self::shortcuts(self.enabled);
        if !enabled.any() {
            self.conflicts = [false; 4];
            let result = self.raw(enabled);
            // Best-effort release of every shortcut even if device removal fails.
            for (i, registered) in before.iter().enumerate() {
                if *registered {
                    unsafe {
                        let _ = UnregisterHotKey(Some(self.owner), HOTKEY_BASE + i as i32);
                    }
                }
            }
            self.enabled = enabled;
            return result;
        }
        let after = Self::shortcuts(enabled);
        let mut added = [false; 4];
        // PITFALL: Reserve the new shortcuts before dropping old input registrations.
        unsafe {
            for (i, needed) in after.iter().enumerate() {
                if *needed && !before[i] {
                    if RegisterHotKey(
                        Some(self.owner),
                        HOTKEY_BASE + i as i32,
                        self.combination(i).0 | MOD_NOREPEAT,
                        self.combination(i).1,
                    )
                    .is_err()
                    {
                        for (j, registered) in added.iter().enumerate() {
                            if *registered {
                                let _ = UnregisterHotKey(Some(self.owner), HOTKEY_BASE + j as i32);
                            }
                        }
                        return Err(Error::new(
                            E_FAIL,
                            format!(
                                "Shortcut {} is already in use. No additional feature was enabled.",
                                self.name(i)
                            ),
                        ));
                    }
                    added[i] = true;
                }
            }
            if let Err(error) = self.raw(enabled) {
                let _ = self.raw(self.enabled);
                for (i, registered) in added.iter().enumerate() {
                    if *registered {
                        let _ = UnregisterHotKey(Some(self.owner), HOTKEY_BASE + i as i32);
                    }
                }
                return Err(error);
            }
            for i in 0..4 {
                if before[i] && !after[i] {
                    let _ = UnregisterHotKey(Some(self.owner), HOTKEY_BASE + i as i32);
                }
            }
        }
        if enabled.keys && self.priority.is_none() {
            self.priority = Some(super::priority::Priority::new(self.owner, [None; 3], true)?);
        }
        self.enabled = enabled;
        Ok(())
    }
    fn combination(&self, index: usize) -> (HOT_KEY_MODIFIERS, u32) {
        let s = if index == 0 {
            self.stop
        } else {
            self.shortcuts[index - 1]
        };
        (HOT_KEY_MODIFIERS(s.modifiers), s.key)
    }
    fn name(&self, index: usize) -> String {
        if index == 0 && self.stop == STOP_SHORTCUT {
            "Ctrl+Alt+Esc".into()
        } else {
            let (mods, key) = self.combination(index);
            format!(
                "{} (modifiers {}, key {})",
                ["Turn everything off", "Arrow", "Rectangle", "Ellipse"][index],
                mods.0,
                key
            )
        }
    }
    pub fn available(&mut self, enabled: Enabled) -> Result<Vec<&'static str>> {
        self.set(Enabled::default())?;
        let mut conflicts = Vec::new();
        let mut priority = [None; 3];
        for (i, needed) in Self::shortcuts(enabled).into_iter().enumerate() {
            if needed {
                let (mods, key) = self.combination(i);
                if let Err(error) = unsafe {
                    RegisterHotKey(
                        Some(self.owner),
                        HOTKEY_BASE + i as i32,
                        mods | MOD_NOREPEAT,
                        key,
                    )
                } {
                    if i > 0
                        && error.code() == HRESULT::from_win32(ERROR_HOTKEY_ALREADY_REGISTERED.0)
                    {
                        priority[i - 1] = Some((self.shortcuts[i - 1], HOTKEY_BASE + i as i32));
                    } else {
                        self.conflicts[i] = true;
                        conflicts.push(["Turn everything off", "Arrow", "Rectangle", "Ellipse"][i]);
                    }
                }
            }
        }
        // Track requested registrations, including failed ones: removing a missing ID is harmless.
        self.enabled = enabled;
        if let Err(error) = self.raw(enabled) {
            let _ = self.set(Enabled::default());
            return Err(error);
        }
        if enabled.keys || priority.iter().any(Option::is_some) {
            match super::priority::Priority::new(self.owner, priority, enabled.keys) {
                Ok(hook) => self.priority = Some(hook),
                Err(error) => {
                    let _ = self.set(Enabled::default());
                    return Err(error);
                }
            }
        }
        Ok(conflicts)
    }
    pub fn priority_active(&self) -> bool {
        self.priority.is_some()
    }
    fn shortcuts(e: Enabled) -> [bool; 4] {
        [e.any(), e.shapes, e.shapes, e.shapes]
    }
    fn raw(&self, enabled: Enabled) -> Result<()> {
        // The diagram needs the mouse even without showing it: Raw Input ends its drag.
        let devices =
            [(2, enabled.halo || enabled.devices()), (6, enabled.keys)].map(|(usage, active)| {
                RAWINPUTDEVICE {
                    usUsagePage: 1,
                    usUsage: usage,
                    dwFlags: if active {
                        RIDEV_INPUTSINK
                    } else {
                        RIDEV_REMOVE
                    },
                    hwndTarget: if active { self.owner } else { HWND::default() },
                }
            });
        unsafe { RegisterRawInputDevices(&devices, std::mem::size_of::<RAWINPUTDEVICE>() as u32) }
    }
}
impl Drop for Bindings {
    fn drop(&mut self) {
        let _ = self.set(Enabled::default());
    }
}

pub(super) fn read_raw(l: LPARAM) -> Result<RAWINPUT> {
    let mut input = RAWINPUT::default();
    let mut size = std::mem::size_of::<RAWINPUT>() as u32;
    // PITFALL: RAWINPUT is aligned, unlike a byte vector; only keyboard and mouse are registered.
    unsafe {
        let read = GetRawInputData(
            HRAWINPUT(l.0 as *mut _),
            RID_INPUT,
            Some((&mut input as *mut RAWINPUT).cast()),
            &mut size,
            std::mem::size_of::<RAWINPUTHEADER>() as u32,
        );
        if read == u32::MAX {
            return Err(Error::from_thread());
        }
        if read < std::mem::size_of::<RAWINPUTHEADER>() as u32 {
            return Err(Error::new(E_FAIL, "Truncated input event"));
        }
    }
    Ok(input)
}

pub(super) fn held_modifiers() -> [bool; 4] {
    unsafe {
        [
            GetAsyncKeyState(VK_CONTROL.0 as i32) < 0,
            GetAsyncKeyState(VK_MENU.0 as i32) < 0,
            GetAsyncKeyState(VK_SHIFT.0 as i32) < 0,
            GetAsyncKeyState(VK_LWIN.0 as i32) < 0 || GetAsyncKeyState(VK_RWIN.0 as i32) < 0,
        ]
    }
}
