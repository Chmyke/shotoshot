//! ROLE: Checks real input registrations and shortcut rollback in a fresh hidden process.
//! INVARIANTS: No simulated input, clipboard, user preferences or visible windows. Every shortcut
//! is picked among free combinations, so a running resident in Presentation mode cannot collide.
use super::*;

fn registered() -> Result<Vec<RAWINPUTDEVICE>> {
    unsafe {
        let mut count = 0;
        if GetRegisteredRawInputDevices(
            None,
            &mut count,
            std::mem::size_of::<RAWINPUTDEVICE>() as u32,
        ) == u32::MAX
        {
            return Err(Error::from_thread());
        }
        let mut devices = vec![RAWINPUTDEVICE::default(); count as usize];
        if count > 0
            && GetRegisteredRawInputDevices(
                Some(devices.as_mut_ptr()),
                &mut count,
                std::mem::size_of::<RAWINPUTDEVICE>() as u32,
            ) == u32::MAX
        {
            return Err(Error::from_thread());
        }
        devices.truncate(count as usize);
        Ok(devices)
    }
}
fn ensure(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(Error::new(E_FAIL, message))
    }
}
pub(crate) fn run() -> Result<()> {
    unsafe {
        let owner = crate::resources::OwnedWindow(CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("STATIC"),
            w!("Shotoshot input probe"),
            WS_POPUP,
            -32000,
            -32000,
            1,
            1,
            None,
            None,
            None,
            None,
        )?);
        ensure(
            registered()?.is_empty(),
            "Fresh process already has raw input",
        )?;
        let mut bindings = Bindings::new(owner.0);
        // Injectable bindings let the regression probe avoid shortcuts owned by other apps,
        // including the installed resident, which holds Ctrl+Alt+Esc while Presentation is on.
        let probe_mods = MOD_CONTROL | MOD_ALT | MOD_SHIFT;
        let mut free = Vec::new();
        for key in VK_F1.0..=VK_F11.0 {
            if RegisterHotKey(None, 198, probe_mods, key as u32).is_ok() {
                UnregisterHotKey(None, 198)?;
                free.push(key as u32);
                if free.len() == 4 {
                    break;
                }
            }
        }
        ensure(
            free.len() == 4,
            "Four free diagnostic combinations are required",
        )?;
        let probe_keys = [free[0], free[1], free[2]];
        bindings.stop = crate::settings::Shortcut {
            modifiers: probe_mods.0,
            key: free[3],
        };
        bindings.shortcuts = probe_keys.map(|key| crate::settings::Shortcut {
            modifiers: probe_mods.0,
            key,
        });
        let mut enabled = Enabled {
            shapes: true,
            ..Default::default()
        };
        bindings.set(enabled)?;
        ensure(
            registered()?.is_empty(),
            "Drawing shortcuts must not monitor any device",
        )?;
        enabled.halo = true;
        bindings.set(enabled)?;
        let devices = registered()?;
        ensure(
            devices.len() == 1 && devices[0].usUsage == 2 && devices[0].dwFlags == RIDEV_INPUTSINK,
            "Halo must observe only the mouse, without suppressing legacy input",
        )?;
        enabled.keys = true;
        bindings.set(enabled)?;
        let devices = registered()?;
        ensure(
            devices.len() == 2
                && devices.iter().all(|device| {
                    device.hwndTarget == owner.0 && device.dwFlags == RIDEV_INPUTSINK
                }),
            "Keys must observe mouse and keyboard on the resident",
        )?;
        enabled.keys = false;
        enabled.shapes = false;
        bindings.set(enabled)?;
        let devices = registered()?;
        ensure(
            devices.len() == 1 && devices[0].usUsage == 2,
            "Disabling keys must unregister the keyboard",
        )?;
        enabled.halo = false;
        enabled.mouse = true;
        bindings.set(enabled)?;
        let devices = registered()?;
        ensure(
            devices.len() == 1 && devices[0].usUsage == 2,
            "The mouse diagram alone must not observe the keyboard",
        )?;
        enabled.halo = true;
        enabled.mouse = false;
        bindings.set(enabled)?;

        // A different thread owns the conflicting shortcut, exactly like an independent app.
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (stop_tx, stop_rx) = std::sync::mpsc::channel();
        let blocker = std::thread::spawn(move || {
            let result = RegisterHotKey(None, 199, probe_mods, probe_keys[1]);
            let active = result.is_ok();
            let _ = ready_tx.send(result);
            let _ = stop_rx.recv_timeout(std::time::Duration::from_secs(5));
            if active {
                let _ = UnregisterHotKey(None, 199);
            }
        });
        let conflict = (|| -> Result<()> {
            ready_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .map_err(|e| Error::new(E_FAIL, e.to_string()))??;
            enabled.shapes = true;
            ensure(
                bindings.set(enabled).is_err(),
                "Occupied shortcut was not rejected",
            )?;
            let devices = registered()?;
            ensure(
                devices.len() == 1 && devices[0].usUsage == 2,
                "Failed activation changed the existing halo registration",
            )?;
            ensure(
                bindings.available(enabled)?.is_empty() && bindings.priority_active(),
                "A conflicting drawing shortcut must receive a priority hook",
            )?;
            ensure(
                bindings.conflicts == [false; 4],
                "A working priority shortcut must not be marked unavailable",
            )?;
            Ok(())
        })();
        let _ = stop_tx.send(());
        blocker
            .join()
            .map_err(|_| Error::new(E_FAIL, "Shortcut probe thread panicked"))?;
        conflict?;
        bindings.set(Enabled::default())?;
        ensure(
            !bindings.priority_active(),
            "Stop must release the priority hook thread",
        )?;
        ensure(registered()?.is_empty(), "Stop left raw input registered")?;
        drop(bindings);
        let released = std::thread::spawn(move || -> Result<()> {
            for (modifiers, key) in [
                (probe_mods, free[3]),
                (probe_mods, probe_keys[0]),
                (probe_mods, probe_keys[1]),
                (probe_mods, probe_keys[2]),
            ] {
                RegisterHotKey(None, 200, modifiers, key)?;
                UnregisterHotKey(None, 200)?;
            }
            Ok(())
        })
        .join()
        .map_err(|_| Error::new(E_FAIL, "Shortcut release probe panicked"))?;
        released?;
    }
    std::fs::write("target/presentation-diagnostics.txt", "PASS: independent raw input, native shortcut rollback, priority hook fallback on collision, hook and registrations released\n")
        .map_err(|e| Error::new(E_FAIL, e.to_string()))
}
