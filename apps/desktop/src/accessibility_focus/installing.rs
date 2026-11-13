#[cfg(target_os = "macos")]
use super::mac;

/// Has GPUI's view answer what has focus with what AccessKit says, for every window.
#[cfg(target_os = "macos")]
pub(crate) fn install() {
    mac::install();
}

/// Nothing elsewhere, since what's left out is in GPUI's view on a Mac.
#[cfg(not(target_os = "macos"))]
pub(crate) fn install() {}
