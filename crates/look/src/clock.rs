//! Where the marks are in their motion. A moving mark is redrawn on every frame of the display, for the smoothest
//! motion it can show. Pictures, and the system's Reduce Motion setting, hold every mark still.

use std::sync::OnceLock;
use std::time::Instant;

use gpui_kit::{App, Global};

static START: OnceLock<Instant> = OnceLock::new();

/// Set by pictures, to hold every mark still.
struct Still;

impl Global for Still {}

/// Whether marks move at all.
pub(crate) fn moving(cx: &App) -> bool {
    !cx.has_global::<Still>() && !cx.reduce_motion()
}

/// Seconds since the first mark asked. Each motion takes its own phase from this.
pub(crate) fn elapsed() -> f64 {
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}

/// Holds every mark still from now on, so pictures of the app come out the same every time.
#[cfg(feature = "testing")]
pub fn hold_still(cx: &mut App) {
    cx.set_global(Still);
}
