//! ROLE: Whether this process runs from the Microsoft Store package (MSIX), and the few system
//! calls that differ there: the package's startup task, the activation kind, the Store's pages.
//! The rules themselves are pure, in `shotoshot_core::store`. Unpackaged (the website's
//! installer), nothing here changes behaviour.
use std::sync::OnceLock;
use windows::{
    ApplicationModel::{Activation::ActivationKind, AppInstance, StartupTask, StartupTaskState},
    Win32::{Foundation::*, Storage::Packaging::Appx::GetCurrentPackageFamilyName},
    core::*,
};

/// The package family name, None outside a package. Asked once: it cannot change.
pub fn family_name() -> Option<&'static str> {
    static FAMILY: OnceLock<Option<String>> = OnceLock::new();
    FAMILY.get_or_init(read_family_name).as_deref()
}

pub fn is_packaged() -> bool {
    family_name().is_some()
}

fn read_family_name() -> Option<String> {
    unsafe {
        let mut length = 0u32;
        // APPMODEL_ERROR_NO_PACKAGE outside a package; ERROR_INSUFFICIENT_BUFFER inside.
        if GetCurrentPackageFamilyName(&mut length, None) != ERROR_INSUFFICIENT_BUFFER {
            return None;
        }
        let mut name = vec![0u16; length as usize];
        if GetCurrentPackageFamilyName(&mut length, Some(PWSTR(name.as_mut_ptr()))) != ERROR_SUCCESS
        {
            return None;
        }
        name.truncate(name.iter().position(|c| *c == 0).unwrap_or(name.len()));
        Some(String::from_utf16_lossy(&name))
    }
}

/// Packaged: whether the user started Shotoshot (Start menu, search), rather than the startup
/// task at sign in. Unpackaged, the Start menu shortcut says so with `--ready` instead.
/// A plain launch may come without activation arguments: only the startup task says "not by hand".
pub fn started_by_hand() -> bool {
    !AppInstance::GetActivatedEventArgs()
        .and_then(|args| args.Kind())
        .is_ok_and(|kind| kind == ActivationKind::StartupTask)
}

fn startup_task() -> Result<StartupTask> {
    StartupTask::GetAsync(&HSTRING::from(shotoshot_core::store::STARTUP_TASK))?.join()
}

/// Packaged: whether the startup task will start Shotoshot at sign in.
pub fn startup_enabled() -> Result<bool> {
    Ok(matches!(
        startup_task()?.State()?,
        StartupTaskState::Enabled | StartupTaskState::EnabledByPolicy
    ))
}

/// Packaged: turns the startup task on or off. A packaged desktop app enables it without a
/// dialog; a task the user turned off in Windows (Task Manager, Settings) stays off, as Windows
/// requires: the switch then reads off again.
pub fn set_startup_enabled(enabled: bool) -> Result<()> {
    let task = startup_task()?;
    if enabled {
        task.RequestEnableAsync()?.join()?;
    } else {
        task.Disable()?;
    }
    Ok(())
}
