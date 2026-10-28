//! ROLE: Drives the pointer effects of the halo layer and the spotlight from native events.
//! OWNER: Presentation, on its resident message loop.
//! INVARIANTS: With the default settings the halo behaves as it always did: it moves with
//! each event and repaints only on clicks. Effects that change every frame (trail, click
//! waves, smooth follow, spotlight, find) paint from the timer, which runs only while they move.
use super::*;
use shotoshot_core::pointer::{Button, ClickStyle, Follow, SpotlightShape, layer_radius};

/// Shortest time between two frames painted from pointer events.
const FRAME_MS: u64 = 12;
/// Hotkey ids after the four of `Bindings`: show a click, hold the spotlight.
pub(super) const POINTER_HOTKEYS: [i32; 2] = [HOTKEY_BASE + 4, HOTKEY_BASE + 5];

/// The screen holding `at`, in device pixels.
fn monitor_rect(at: POINT) -> Result<RECT> {
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    unsafe {
        GetMonitorInfoW(MonitorFromPoint(at, MONITOR_DEFAULTTONEAREST), &mut info).ok()?;
    }
    Ok(info.rcMonitor)
}
fn union(a: Option<RECT>, b: Option<RECT>) -> Option<RECT> {
    match (a, b) {
        (Some(a), Some(b)) => Some(RECT {
            left: a.left.min(b.left),
            top: a.top.min(b.top),
            right: a.right.max(b.right),
            bottom: a.bottom.max(b.bottom),
        }),
        (a, b) => a.or(b),
    }
}
fn point(p: POINT) -> Point {
    Point {
        x: p.x as f64,
        y: p.y as f64,
    }
}

impl Presentation {
    /// Whether the halo layer is painted from frames rather than from clicks.
    pub(super) fn frame_driven(&self, now: u64) -> bool {
        let settings = &self.preferences.pointer;
        settings.draws_around()
            || settings.follow == Follow::Smooth
            || settings.spotlight != SpotlightShape::Off
            || self.effects.finding(now).is_some()
    }
    /// Side of the square halo layer at `at`, in device pixels. Rounded up to 32 pixels: a
    /// size slider dragged in the pointer window rebuilds the layer only now and then.
    pub(super) fn halo_size(&self, at: POINT, now: u64) -> i32 {
        let finding = self.effects.finding(now).is_some();
        let side = 2. * layer_radius(&self.preferences.pointer, finding) * monitor_scale(at);
        (side / 32.).ceil() as i32 * 32
    }
    /// A mouse event while the halo is on: `flags` are the raw button flags.
    pub(super) fn pointer_event(&mut self, flags: u16) -> Result<()> {
        let now = self.now();
        let at = cursor()?;
        let scale = monitor_scale(at);
        let settings = self.preferences.pointer.clone();
        self.effects.moved(point(at), now, scale, &settings);
        for (index, button) in [Button::Left, Button::Right, Button::Middle]
            .into_iter()
            .enumerate()
        {
            if flags & (1 << (index * 2)) != 0 {
                if settings.click != ClickStyle::Off {
                    self.effects.clicked(point(at), button, now);
                }
                sound::play(settings.sound, settings.volume);
            }
        }
        if self.frame_driven(now) {
            self.pointer_dirty = true;
            // A frame due already is painted now rather than at the next timer tick: the
            // ribbon and the halo stay with the pointer instead of lagging behind it.
            if now.saturating_sub(self.last_frame) >= FRAME_MS {
                self.pointer_frame()?;
            }
            self.sync_timer()
        } else {
            self.move_halo()
        }
    }
    /// One frame of the frame-driven effects: place the layer, paint it, light the spotlight.
    pub(super) fn pointer_frame(&mut self) -> Result<()> {
        if !self.enabled.halo {
            return Ok(());
        }
        let now = self.now();
        let at = cursor()?;
        let settings = self.preferences.pointer.clone();
        self.effects.step(now, &settings);
        // One surface over the pointer's screen that never moves: only the area the effects
        // touch is redrawn and sent. A small window that followed the pointer would show, for a
        // frame, its new place with its old pixels (the trail jumped), and cut the trail off.
        let screen = monitor_rect(at)?;
        let (width, height) = (screen.right - screen.left, screen.bottom - screen.top);
        let stale = self.halo.as_ref().is_none_or(|halo| {
            halo.width != width || halo.height != height || self.halo_screen != screen
        });
        if stale {
            let layer = Layer::new(self.address, screen.left, screen.top, width, height)?;
            self.halo = Some(self.mirrored(layer, Slot::Halo));
            self.halo_screen = screen;
            self.last_bounds = None;
            if let Some(halo) = &self.halo {
                halo.position(screen.left, screen.top)?;
                halo.show(true);
            }
        }
        self.halo_origin = POINT {
            x: screen.left,
            y: screen.top,
        };
        self.halo_scale = monitor_scale(at);
        let origin = point(self.halo_origin);
        let local = self
            .effects
            .bounds(now, &settings, self.halo_scale)
            .map(|r| RECT {
                left: (r.min.x - origin.x).floor() as i32,
                top: (r.min.y - origin.y).floor() as i32,
                right: (r.max.x - origin.x).ceil() as i32,
                bottom: (r.max.y - origin.y).ceil() as i32,
            });
        let dirty = union(local, self.last_bounds).map(|r| RECT {
            left: r.left.clamp(0, width),
            top: r.top.clamp(0, height),
            right: r.right.clamp(0, width),
            bottom: r.bottom.clamp(0, height),
        });
        if let (Some(halo), Some(dirty)) = (&self.halo, dirty) {
            halo.begin_region(dirty)?;
            let drawn = halo.pointer(
                origin,
                &self.effects,
                &settings,
                self.halo_look(now),
                self.halo_scale,
                now,
            );
            drawn.and(halo.end_region(dirty))?;
        }
        self.last_bounds = local;
        if settings.spotlight == SpotlightShape::Off {
            self.spotlight = None;
        } else {
            if self.spotlight.is_none() {
                self.spotlight = Some(spotlight::Spotlight::new()?);
            }
            let lit = self
                .effects
                .spotlight_lit(now, self.spotlight_held, &settings);
            if let Some(spotlight) = &mut self.spotlight {
                spotlight.update(lit, at, &settings, monitor_scale(at))?;
            }
        }
        self.pointer_dirty = false;
        self.last_frame = now;
        Ok(())
    }
    /// Whether the pointer effects need the timer now.
    pub(super) fn pointer_animating(&self, now: u64) -> bool {
        self.enabled.halo
            && (self.pointer_dirty
                || self.spotlight_held
                || self.effects.animating(now, &self.preferences.pointer))
    }
    /// A pointer shortcut: 0 shows a click at the pointer, 1 holds the spotlight.
    pub(super) fn pointer_hotkey(&mut self, index: usize) -> Result<()> {
        if !self.enabled.halo {
            return Ok(());
        }
        let now = self.now();
        let at = cursor()?;
        if index == 0 {
            self.effects.clicked(point(at), Button::Left, now);
        } else {
            self.spotlight_held = true;
        }
        self.pointer_dirty = true;
        self.sync_timer()
    }
    /// Ends a held spotlight once its key is up: a registered shortcut reports only presses.
    pub(super) fn check_spotlight_key(&mut self) {
        if !self.spotlight_held {
            return;
        }
        let held = self
            .preferences
            .pointer
            .spotlight_shortcut
            .is_some_and(|shortcut| unsafe { GetAsyncKeyState(shortcut.key as i32) } < 0);
        if !held {
            self.spotlight_held = false;
            self.pointer_dirty = true;
        }
    }
    /// Registers the pointer shortcuts the settings name, while the halo is on.
    pub(super) fn sync_pointer_keys(&mut self) {
        self.release_pointer_keys();
        if !self.enabled.halo {
            return;
        }
        let shortcuts = [
            self.preferences.pointer.click_shortcut,
            self.preferences.pointer.spotlight_shortcut,
        ];
        for (index, shortcut) in shortcuts.into_iter().enumerate() {
            let Some(shortcut) = shortcut else {
                continue;
            };
            let registered = unsafe {
                RegisterHotKey(
                    Some(self.owner),
                    POINTER_HOTKEYS[index],
                    HOT_KEY_MODIFIERS(shortcut.modifiers) | MOD_NOREPEAT,
                    shortcut.key,
                )
            };
            self.pointer_keys[index] = registered.is_ok();
            self.pointer_conflicts[index] = registered.is_err();
        }
    }
    pub(super) fn release_pointer_keys(&mut self) {
        for (index, registered) in self.pointer_keys.iter_mut().enumerate() {
            if *registered {
                unsafe {
                    let _ = UnregisterHotKey(Some(self.owner), POINTER_HOTKEYS[index]);
                }
                *registered = false;
            }
        }
        self.pointer_conflicts = [false; 2];
        self.spotlight_held = false;
    }
}
