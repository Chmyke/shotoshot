//! ROLE: Windows platform layer of Shotoshot (Win32, Direct2D, WIC); empty elsewhere.
#![cfg(windows)]
mod app;
pub mod board;
use shotoshot_ui::capture_name;
mod demo;
mod diagnostics;
mod export;
pub mod failure;
mod game_capture;
mod google_search;
mod hotkey;
mod image;
mod language;
mod presentation;
mod print_screen;
mod rename_migration;
mod render;
mod resources;
mod session;
pub mod settings;
mod site_previews;
mod startup;
#[cfg(test)]
mod text_fit;
mod toolbar;
pub use app::{reports_errors_on_stderr, run};
pub use export::*;
pub use image::*;
pub use render::*;

pub struct ComApartment(std::marker::PhantomData<std::rc::Rc<()>>);
impl ComApartment {
    pub fn new() -> windows::core::Result<Self> {
        keep_com_loaded();
        // PITFALL: COM is initialized and released on the same calling thread.
        unsafe {
            windows::Win32::System::Com::CoInitializeEx(
                None,
                windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
            )
            .ok()?;
        }
        Ok(Self(std::marker::PhantomData))
    }
}
impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe {
            windows::Win32::System::Com::CoUninitialize();
        }
    }
}
/// Keeps COM, and the WinRT libraries it loaded, in the process until it ends.
/// PITFALL: windows-rs caches WinRT factories (the capture frame pool's among them) for the whole
/// process. When the last apartment closes, COM unloads their libraries and the next call through
/// the cache crashes (2026-10-29: `STATUS_ACCESS_VIOLATION` in the loupe's real-desktop test,
/// up to one series in two, after an earlier test closed its apartment).
fn keep_com_loaded() {
    static KEPT: std::sync::Once = std::sync::Once::new();
    KEPT.call_once(|| unsafe {
        // Never released: the cookie only matters to give the usage back.
        let _ = windows::Win32::System::Com::CoIncrementMTAUsage();
    });
}

#[cfg(test)]
mod tests {
    use windows::Graphics::Capture::GraphicsCaptureSession;

    /// A WinRT factory windows-rs cached while an apartment was open still works once every
    /// apartment of the process has closed. Run in a process of its own: another test's
    /// apartment would keep COM loaded and hide the fault.
    #[test]
    fn a_cached_winrt_factory_outlives_the_apartment_that_loaded_it() {
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "tests::cached_factory_after_the_last_apartment",
                "--ignored",
                "--test-threads=1",
            ])
            .output()
            .unwrap();
        assert!(
            child.status.success() && String::from_utf8_lossy(&child.stdout).contains("1 passed"),
            "{:?}
{}",
            child.status,
            String::from_utf8_lossy(&child.stderr)
        );
    }
    #[test]
    #[ignore = "run alone in its own process by the test above"]
    fn cached_factory_after_the_last_apartment() {
        std::thread::spawn(|| {
            let _com = super::ComApartment::new().unwrap();
            GraphicsCaptureSession::IsSupported().unwrap();
        })
        .join()
        .unwrap();
        // No apartment left: before the fix, COM had unloaded GraphicsCapture.dll here and
        // this call through the cached factory crashed the process.
        std::thread::spawn(|| GraphicsCaptureSession::IsSupported().unwrap())
            .join()
            .unwrap();
    }
}
