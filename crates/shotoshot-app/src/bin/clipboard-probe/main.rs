//! Windows-only clipboard probe; other platforms build an empty binary.
#[cfg(windows)]
mod probe;
#[cfg(windows)]
fn main() {
    probe::main()
}
#[cfg(not(windows))]
fn main() {}
