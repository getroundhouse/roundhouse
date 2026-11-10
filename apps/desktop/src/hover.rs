//! How a hover card times itself, the same for every one in the app.

use std::time::Duration;

/// How long the pointer rests before a card shows: as long as GPUI waits before any other tooltip in the app shows,
/// so a card doesn't feel slower than one.
pub(crate) const OPENS_AFTER: Duration = Duration::from_millis(500);
/// A card goes as soon as the pointer does, as a tooltip does.
pub(crate) const CLOSES_AFTER: Duration = Duration::ZERO;
