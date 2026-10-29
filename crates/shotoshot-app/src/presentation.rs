//! ROLE: Owns opt-in desktop presentation overlays and input subscriptions.
//! OWNER: Resident main thread; no registrations or surfaces exist while disabled.
//! INVARIANTS: Completing or cancelling a gesture restores click-through immediately.
mod broadcast;
mod devices;
mod diagram;
mod events;
mod halo;
pub(crate) use events::resident_message;
pub(crate) mod demo;
#[cfg(test)]
mod display_tests;
mod input;
mod layer;
mod layout;
mod loupe;
mod loupe_capture;
pub(crate) mod loupe_probe;
mod manipulation;
pub(crate) mod menu;
pub(crate) mod model;
mod pointer_effects;
mod priority;
mod priority_match;
pub(crate) mod probe;
mod sound;
mod spotlight;
#[cfg(test)]
mod tests;

use crate::settings::PresentationSettings;
use broadcast::{Broadcast, Slot};
use devices::Devices;
#[cfg(test)]
pub(crate) use input::SHORTCUT_NAMES;
use input::{Bindings, held_modifiers};
use layer::Layer;
use model::{LiveTool, Scene};
use shotoshot_core::Point;
use shotoshot_ui::wheel;
use std::{cell::RefCell, rc::Rc, time::Instant};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::*,
        UI::{HiDpi::*, Input::KeyboardAndMouse::*, Input::*, WindowsAndMessaging::*},
    },
    core::*,
};

pub(crate) const HOTKEY_BASE: i32 = 90;
pub(crate) const TIMER: usize = 90;
pub(crate) const FAILED: u32 = WM_APP + 10;
const OBSERVED_KEY: u32 = WM_APP + 12;
pub(crate) const PREFERENCES_CHANGED: u32 = WM_APP + 11;
const STATE_PROPERTY: PCWSTR = w!("Shotoshot.PresentationState");

#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) struct Enabled {
    pub halo: bool,
    /// The live loupe, apart from the halo: either may be on alone.
    pub loupe: bool,
    pub shapes: bool,
    /// The keyboard of the input diagram.
    pub keys: bool,
    /// The mouse of the input diagram.
    pub mouse: bool,
}
impl Enabled {
    pub fn any(self) -> bool {
        self.pointer() || self.shapes || self.devices()
    }
    /// Whether the pointer layer follows the pointer: for the halo, the loupe, or both.
    pub fn pointer(self) -> bool {
        self.halo || self.loupe
    }
    /// Whether the input diagram shows anything.
    pub fn devices(self) -> bool {
        self.keys || self.mouse
    }
}

pub(crate) struct Presentation {
    owner: HWND,
    address: *const RefCell<Self>,
    pub enabled: Enabled,
    pub preferences: PresentationSettings,
    bindings: Bindings,
    halo: Option<Layer>,
    canvas: Option<Layer>,
    keys: Option<Layer>,
    lock: Option<Layer>,
    broadcast: Option<Rc<Broadcast>>,
    input_drag: Option<(POINT, POINT)>,
    /// The whole diagram is shown and draggable, until its lock, Enter or Esc ends the move.
    /// Otherwise only pressed keys and buttons appear and every click passes through.
    pub moving: bool,
    wheel_travel: wheel::Notches,
    scene: Scene,
    origin: POINT,
    previous: HWND,
    clock: Instant,
    timer: bool,
    devices: Devices,
    input_monitor: POINT,
    halo_state: halo::HaloState,
    capture_resume: Option<(Enabled, POINT)>,
    /// Trail, click waves and smooth follow of the halo.
    effects: shotoshot_core::pointer::Effects,
    /// The halo layer's screen position and the monitor scale it was painted for.
    halo_origin: POINT,
    halo_scale: f64,
    /// A pointer event waits for the next frame.
    pointer_dirty: bool,
    spotlight: Option<spotlight::Spotlight>,
    /// The spotlight's shortcut is held down.
    spotlight_held: bool,
    /// Which pointer shortcuts are registered (show a click, hold the spotlight, the loupe).
    pointer_keys: [bool; 3],
    /// Pointer shortcuts another program holds.
    pub pointer_conflicts: [bool; 3],
    /// The loupe's shortcut, in its hold mode, is held down.
    loupe_held: bool,
    /// The magnifier window, while the loupe is on screen.
    loupe: Option<loupe::Loupe>,
    /// Period of the running timer, in milliseconds.
    timer_ms: u32,
    /// When the last frame-driven frame was painted.
    last_frame: u64,
    /// The screen the frame-driven halo surface covers, and the area its last frame drew.
    halo_screen: RECT,
    last_bounds: Option<RECT>,
}
impl Presentation {
    pub fn new(owner: HWND, preferences: PresentationSettings) -> Result<Box<RefCell<Self>>> {
        let state = Box::new(RefCell::new(Self {
            owner,
            address: std::ptr::null(),
            enabled: Enabled::default(),
            preferences,
            bindings: Bindings::new(owner),
            halo: None,
            canvas: None,
            keys: None,
            lock: None,
            broadcast: None,
            input_drag: None,
            moving: false,
            wheel_travel: Default::default(),
            scene: Scene::default(),
            origin: POINT::default(),
            previous: HWND::default(),
            clock: Instant::now(),
            timer: false,
            devices: Devices::default(),
            input_monitor: POINT::default(),
            halo_state: halo::HaloState::default(),
            capture_resume: None,
            effects: Default::default(),
            halo_origin: POINT::default(),
            halo_scale: 1.,
            pointer_dirty: false,
            spotlight: None,
            spotlight_held: false,
            pointer_keys: [false; 3],
            pointer_conflicts: [false; 3],
            loupe_held: false,
            loupe: None,
            timer_ms: 0,
            last_frame: 0,
            halo_screen: RECT::default(),
            last_bounds: None,
        }));
        state.borrow_mut().address = &*state;
        {
            let mut state = state.borrow_mut();
            state.bindings.shortcuts = state.preferences.shape_shortcuts;
        }
        // PITFALL: This independent pointer keeps input and timers alive inside a modal tray menu.
        unsafe {
            SetPropW(
                owner,
                STATE_PROPERTY,
                Some(HANDLE((&*state as *const RefCell<Self>).cast_mut().cast())),
            )?;
        }
        Ok(state)
    }
    fn now(&self) -> u64 {
        self.clock.elapsed().as_millis().min(u64::MAX as u128) as u64
    }
    pub fn suspend_capture(&mut self) -> Result<()> {
        if self.capture_resume.is_none() {
            let previous = (self.enabled, self.input_monitor);
            self.stop()?;
            self.capture_resume = Some(previous);
        }
        Ok(())
    }
    pub fn resume_capture(&mut self) -> Result<()> {
        if let Some((enabled, monitor)) = self.capture_resume.take() {
            self.set_enabled(enabled)?;
            if enabled.devices() {
                self.input_monitor = monitor;
                self.show_devices()?;
            }
        }
        Ok(())
    }
    pub fn display_changed(&mut self) -> Result<()> {
        // A capture already released every surface. Keep its saved choices until it exits.
        if self.capture_resume.is_some() {
            return Ok(());
        }
        // Rebuild only graphics: shortcut setup may have suspended the existing bindings.
        let result = (|| {
            self.clear_surfaces()?;
            self.move_halo()?;
            if self.enabled.devices() {
                self.show_devices()?;
            }
            self.sync_timer()
        })();
        if result.is_err() {
            let _ = self.stop();
        }
        result
    }
    pub fn set_enabled(&mut self, enabled: Enabled) -> Result<()> {
        if !enabled.any() {
            return self.stop();
        }
        let new_halo = if enabled.pointer() && self.halo.is_none() {
            let point = cursor()?;
            self.effects.place(
                Point {
                    x: point.x as f64,
                    y: point.y as f64,
                },
                self.now(),
            );
            let size = self.halo_size(point);
            Some(self.mirrored(
                Layer::new(
                    self.address,
                    point.x - size / 2,
                    point.y - size / 2,
                    size,
                    size,
                )?,
                Slot::Halo,
            ))
        } else {
            None
        };
        if let Err(error) = self.bindings.available(enabled) {
            let _ = self.stop();
            return Err(error);
        }
        if (enabled.keys, enabled.mouse) != (self.enabled.keys, self.enabled.mouse) {
            // A device that appears or leaves starts from released keys and buttons.
            self.devices.clear();
        }
        if enabled.keys && !self.enabled.keys {
            unsafe {
                self.devices.ansi = layout::is_ansi(GetKeyboardLayout(GetWindowThreadProcessId(
                    GetForegroundWindow(),
                    None,
                )));
            }
        }
        if enabled.devices() && !self.enabled.devices() {
            self.input_monitor = cursor()?;
        }
        self.enabled = enabled;
        if let Some(halo) = new_halo {
            self.halo = Some(halo);
        }
        if !enabled.pointer() {
            self.halo = None;
            self.halo_state.clear();
            self.effects.clear();
        }
        if !enabled.halo {
            self.spotlight = None;
        }
        self.sync_pointer_keys();
        if !enabled.devices() {
            self.end_input_drag(false)?;
            self.moving = false;
            self.keys = None;
            self.lock = None;
        }
        if !enabled.shapes {
            self.release_gesture()?;
            self.scene.clear();
            self.canvas = None;
        }
        let result = (|| {
            if enabled.devices() {
                self.show_devices()?;
            }
            self.move_halo()?;
            if let Some(halo) = &self.halo {
                self.paint(halo.hwnd())?;
            }
            if self.enabled.pointer() && self.frame_driven() {
                self.pointer_frame()?;
            }
            self.sync_loupe()?;
            self.sync_timer()
        })();
        if result.is_err() {
            let _ = self.stop();
        }
        result
    }
    pub fn stop(&mut self) -> Result<()> {
        self.capture_resume = None;
        self.enabled = Enabled::default();
        let registrations = self.bindings.set(self.enabled);
        registrations.and(self.clear_surfaces())
    }
    fn clear_surfaces(&mut self) -> Result<()> {
        let drag = self.end_input_drag(false);
        let release = self.release_gesture();
        self.scene.clear();
        self.canvas = None;
        self.halo = None;
        self.keys = None;
        self.lock = None;
        self.broadcast = None;
        self.input_drag = None;
        self.moving = false;
        self.devices.clear();
        self.halo_state.clear();
        self.effects.clear();
        self.spotlight = None;
        self.loupe = None;
        self.loupe_held = false;
        self.pointer_dirty = false;
        self.last_bounds = None;
        self.release_pointer_keys();
        if self.timer {
            unsafe {
                let _ = KillTimer(Some(self.owner), TIMER);
            }
        }
        self.timer = false;
        release.and(drag)
    }
    pub fn update_preferences(&mut self, preferences: PresentationSettings) -> Result<()> {
        self.bindings.shortcuts = preferences.shape_shortcuts;
        let shortcuts = |p: &PresentationSettings| p.pointer.shortcuts();
        let keys_changed = shortcuts(&self.preferences) != shortcuts(&preferences);
        self.preferences = preferences;
        if self.enabled.devices() {
            let result = self.show_devices();
            if result.is_err() {
                let _ = self.stop();
            }
            result?;
        }
        if self.enabled.pointer() {
            if keys_changed {
                self.sync_pointer_keys();
            }
            let result = (|| {
                if self.preferences.pointer.spotlight
                    == shotoshot_core::pointer::SpotlightShape::Off
                {
                    self.spotlight = None;
                }
                self.move_halo()?;
                if self.frame_driven() {
                    self.pointer_frame()?;
                } else if let Some(halo) = &self.halo {
                    self.paint(halo.hwnd())?;
                }
                self.sync_loupe()?;
                self.sync_timer()
            })();
            if result.is_err() {
                let _ = self.stop();
            }
            result?;
        }
        Ok(())
    }
    pub fn suspend_shortcuts(&mut self) -> Result<()> {
        // The shortcut window records keys: the pointer shortcuts must not answer meanwhile.
        self.release_pointer_keys();
        self.release_gesture()?;
        self.bindings.set(Enabled {
            shapes: false,
            ..self.enabled
        })
    }
    pub fn resume_shortcuts(&mut self) -> Result<Vec<shotoshot_text::Key>> {
        self.sync_pointer_keys();
        let result = self.bindings.available(self.enabled);
        if result.is_err() {
            let _ = self.stop();
        }
        result
    }
    pub fn start_shapes(&mut self) -> Result<Vec<shotoshot_text::Key>> {
        self.enabled.shapes = true;
        self.resume_shortcuts()
    }
    pub fn arm(&mut self, tool: LiveTool) -> Result<()> {
        let result = self.arm_inner(tool);
        if result.is_err() {
            let _ = self.stop();
        }
        result
    }
    pub fn remember_foreground(&mut self, hwnd: HWND) {
        if hwnd != self.owner
            && !self
                .canvas
                .as_ref()
                .is_some_and(|canvas| canvas.hwnd() == hwnd)
        {
            self.previous = hwnd;
        }
    }
    pub fn shortcut_conflicts(&self) -> [bool; 3] {
        [
            self.bindings.conflicts[1],
            self.bindings.conflicts[2],
            self.bindings.conflicts[3],
        ]
    }
    fn arm_inner(&mut self, tool: LiveTool) -> Result<()> {
        if !self.enabled.shapes {
            return Ok(());
        }
        unsafe {
            if self.canvas.is_none() {
                self.origin = POINT {
                    x: GetSystemMetrics(SM_XVIRTUALSCREEN),
                    y: GetSystemMetrics(SM_YVIRTUALSCREEN),
                };
                let canvas = Layer::new(
                    self.address,
                    self.origin.x,
                    self.origin.y,
                    GetSystemMetrics(SM_CXVIRTUALSCREEN),
                    GetSystemMetrics(SM_CYVIRTUALSCREEN),
                )?;
                self.canvas = Some(self.mirrored(canvas, Slot::Canvas));
            }
            let canvas = self
                .canvas
                .as_ref()
                .ok_or_else(|| Error::new(E_FAIL, "Missing drawing surface"))?;
            if GetForegroundWindow() != canvas.hwnd() && GetForegroundWindow() != self.owner {
                self.previous = GetForegroundWindow();
            }
            self.scene.arm(tool);
            canvas.intercept(true)?;
            self.paint(canvas.hwnd())?;
            canvas.show(true);
            if SetForegroundWindow(canvas.hwnd()).as_bool() {
                let _ = SetFocus(Some(canvas.hwnd()));
            }
            // A priority hook does not grant foreground permission. Keep the topmost canvas
            // armed: the first real click activates it normally and starts the gesture.
        }
        Ok(())
    }
    fn release_gesture(&mut self) -> Result<()> {
        self.scene.cancel();
        if let Some(canvas) = &self.canvas {
            canvas.intercept(false)?;
            unsafe {
                if GetCapture() == canvas.hwnd() {
                    let _ = ReleaseCapture();
                }
                if GetForegroundWindow() == canvas.hwnd() && !self.previous.is_invalid() {
                    let _ = SetForegroundWindow(self.previous);
                }
            }
            canvas.invalidate();
            if self.scene.shapes.is_empty() {
                canvas.show(false);
            }
        }
        if self.scene.shapes.is_empty() {
            self.canvas = None;
        }
        Ok(())
    }
    fn sync_timer(&mut self) -> Result<()> {
        let now = self.now();
        let pointer = self.pointer_animating(now);
        let needed = pointer
            || !self.scene.shapes.is_empty()
            || self.devices.needs_animation(now)
            || self.enabled.pointer() && self.halo_state.needs_animation(now);
        // Effects that follow the pointer get every display frame; fades get 30 per second.
        let period = if pointer { 16 } else { 33 };
        if needed && (!self.timer || self.timer_ms != period) {
            unsafe {
                if SetTimer(Some(self.owner), TIMER, period, None) == 0 {
                    return Err(Error::from_thread());
                }
            }
            self.timer = true;
            self.timer_ms = period;
        } else if !needed && self.timer {
            unsafe {
                let _ = KillTimer(Some(self.owner), TIMER);
            }
            self.timer = false;
        }
        Ok(())
    }
    pub fn tick(&mut self) -> Result<()> {
        let now = self.now();
        let removed = self.scene.expire(now);
        if let Some(canvas) = &self.canvas {
            if self.scene.shapes.is_empty() && !self.scene.intercepts_input() {
                self.canvas = None;
            } else if removed
                || self
                    .scene
                    .shapes
                    .iter()
                    .any(|shape| shape.opacity(now) < 1.)
            {
                canvas.invalidate();
            }
        }
        if let Some(keys) = &self.keys {
            keys.invalidate();
        }
        self.check_spotlight_key();
        self.check_loupe_key()?;
        if self.enabled.pointer() && (self.pointer_dirty || self.frame_driven()) {
            self.pointer_frame()?;
        } else if let Some(halo) = &self.halo {
            halo.invalidate();
        }
        self.sync_timer()
    }
    fn move_halo(&mut self) -> Result<()> {
        // Frame-driven effects keep a surface that stays put (`pointer_frame`).
        if !self.enabled.pointer() || self.frame_driven() {
            return Ok(());
        }
        let point = cursor()?;
        let size = self.halo_size(point);
        if self.halo.as_ref().is_none_or(|halo| halo.width != size) {
            let halo = Layer::new(
                self.address,
                point.x - size / 2,
                point.y - size / 2,
                size,
                size,
            )?;
            self.halo = Some(self.mirrored(halo, Slot::Halo));
            if let Some(halo) = &self.halo {
                self.paint(halo.hwnd())?;
            }
        }
        self.halo_origin = POINT {
            x: point.x - size / 2,
            y: point.y - size / 2,
        };
        self.halo_scale = monitor_scale(point);
        if let Some(halo) = &self.halo {
            halo.position(self.halo_origin.x, self.halo_origin.y)?;
            halo.show(true);
        }
        self.sync_loupe()
    }
    /// What the input diagram shows now.
    fn plan(&self) -> diagram::Plan {
        diagram::Plan {
            keyboard: self.enabled.keys,
            mouse: self.enabled.mouse,
        }
    }
    fn show_devices(&mut self) -> Result<()> {
        let point = self.input_monitor;
        let monitor = monitor_scale(point);
        let scale = monitor * self.preferences.input_scale as f64 / 100.;
        let plan = self.plan();
        let width = (plan.width() as f64 * scale) as i32;
        let height = (diagram::HEIGHT as f64 * scale) as i32;
        if self
            .keys
            .as_ref()
            .is_none_or(|layer| layer.width != width || layer.plan() != plan)
        {
            let keys = Layer::diagram(self.address, plan, scale)?;
            self.keys = Some(self.mirrored(keys, Slot::Keys));
        }
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        unsafe {
            GetMonitorInfoW(MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST), &mut info).ok()?;
        }
        if let Some(keys) = &self.keys {
            let margin = (16. * monitor) as i32;
            let position = self.preferences.input_position.index();
            let available = (info.rcWork.right - info.rcWork.left - width).max(0);
            let x = match position % 3 {
                0 => margin.min(available),
                1 => available / 2,
                _ => (available - margin).max(0),
            };
            let available_y = (info.rcWork.bottom - info.rcWork.top - height).max(0);
            let y = if position < 3 {
                margin.min(available_y)
            } else {
                (available_y - margin).max(0)
            };
            let (x, y) = if let Some(offset) = self.preferences.input_offset {
                (
                    (available as i64 * offset[0] as i64 / 10000) as i32,
                    (available_y as i64 * offset[1] as i64 / 10000) as i32,
                )
            } else {
                (x, y)
            };
            keys.mouse_interactive(self.moving)?;
            keys.position(info.rcWork.left + x, info.rcWork.top + y)?;
            self.paint(keys.hwnd())?;
            keys.show(true);
        }
        self.show_lock()?;
        self.sync_timer()
    }
    /// Attaches a viewer-facing layer to the OBS mirror, opening the mirror on first use.
    fn mirrored(&mut self, layer: Layer, slot: Slot) -> Layer {
        if self.broadcast.is_none() {
            // The screen overlays stay useful without the mirror, e.g. with no GPU device.
            self.broadcast = Broadcast::new(self.owner).ok().map(Rc::new);
        }
        layer.mirrored(self.broadcast.clone(), slot)
    }
    fn paint(&self, hwnd: HWND) -> Result<()> {
        let now = self.now();
        if let Some(halo) = &self.halo
            && halo.hwnd() == hwnd
        {
            halo.begin(false)?;
            halo.mark_all();
            let drawn = halo.pointer(
                Point {
                    x: self.halo_origin.x as f64,
                    y: self.halo_origin.y as f64,
                },
                &self.effects,
                &self.pointer_settings(),
                self.halo_look(now),
                self.halo_scale,
                now,
            );
            return drawn.and(halo.end());
        }
        if let Some(keys) = &self.keys
            && keys.hwnd() == hwnd
        {
            keys.begin(self.moving)?;
            keys.devices(&self.devices, now, self.moving);
            return keys.end();
        }
        if let Some(lock) = &self.lock
            && lock.hwnd() == hwnd
        {
            lock.begin(true)?;
            lock.lock_icon(false);
            return lock.end();
        }
        if let Some(canvas) = &self.canvas
            && canvas.hwnd() == hwnd
        {
            canvas.begin(self.scene.intercepts_input())?;
            for shape in &self.scene.shapes {
                canvas.object(&shape.object, shape.opacity(now), shape.arrow_size);
            }
            if let Some(draft) = &self.scene.draft {
                canvas.object(draft, 1., self.scene.arrow_size);
            }
            return canvas.end();
        }
        Ok(())
    }
}
impl Drop for Presentation {
    fn drop(&mut self) {
        // PITFALL: Teardown can dispatch focus/capture messages. Detach every callback before
        // touching window state, because RefCell is not borrowed during its inner Drop.
        unsafe {
            let _ = RemovePropW(self.owner, STATE_PROPERTY);
        }
        for layer in [&self.canvas, &self.halo, &self.keys, &self.lock]
            .into_iter()
            .flatten()
        {
            layer.detach();
        }
        let _ = self.release_gesture();
        let _ = self.end_input_drag(false);
        self.release_pointer_keys();
        if self.timer {
            unsafe {
                let _ = KillTimer(Some(self.owner), TIMER);
            }
        }
    }
}
/// Plays the click sound `sound` at `volume`, as a click on the desktop would.
pub(crate) fn play_click_sound(
    sound: shotoshot_core::pointer::ClickSound,
    volume: shotoshot_core::pointer::Volume,
) {
    sound::play(sound, volume);
}
impl Presentation {
    /// The halo's colour, held button and click pulse at `now`.
    fn halo_look(&self, now: u64) -> shotoshot_ui::pointer::HaloState {
        shotoshot_ui::pointer::HaloState {
            color: self.preferences.halo_color,
            pressed: self.halo_state.pressed(),
            pulse: self
                .halo_state
                .needs_animation(now)
                .then(|| self.halo_state.pulse(now) as f64),
        }
    }
}
#[cfg(test)]
thread_local! {
    /// Where a test on the real desktop says the pointer is, on the probe's screen: it never
    /// moves the real one, and the user's pointer lies on the screen in use.
    static POINTER: std::cell::Cell<Option<POINT>> = const { std::cell::Cell::new(None) };
}
fn cursor() -> Result<POINT> {
    #[cfg(test)]
    if let Some(point) = POINTER.get() {
        return Ok(point);
    }
    let mut point = POINT::default();
    unsafe {
        GetCursorPos(&mut point)?;
    }
    Ok(point)
}
fn monitor_scale(point: POINT) -> f64 {
    let mut x = 96;
    let mut y = 96;
    unsafe {
        let _ = GetDpiForMonitor(
            MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST),
            MDT_EFFECTIVE_DPI,
            &mut x,
            &mut y,
        );
    }
    x as f64 / 96.
}
unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    // PITFALL: Never unwind or hold overlapping mutable borrows across reentrant Win32 calls.
    let result = std::panic::catch_unwind(|| unsafe {
        if msg == WM_NCCREATE {
            let create = &*(l.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<Presentation>;
        if !pointer.is_null()
            && let Ok(mut state) = (*pointer).try_borrow_mut()
        {
            match state.handle(hwnd, msg, w, l) {
                Ok(Some(result)) => return result,
                Ok(None) => {}
                Err(_) => {
                    let _ = state.stop();
                    let _ = PostMessageW(Some(state.owner), FAILED, WPARAM(0), LPARAM(0));
                    return LRESULT(0);
                }
            }
        }
        DefWindowProcW(hwnd, msg, w, l)
    });
    result.unwrap_or_else(|_| {
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
            PostQuitMessage(1);
        }
        LRESULT(0)
    })
}
