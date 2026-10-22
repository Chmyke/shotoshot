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
