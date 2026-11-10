//! Waiting for what shows once the pointer has rested on something.

#![cfg(test)]

use std::time::Duration;

use super::Ui;

/// Longer than the app waits to show a tooltip or a hover card, and to hide one the pointer can move onto.
const LONGER_THAN_A_TOOLTIP_WAITS: Duration = Duration::from_secs(1);

impl Ui<'_> {
    /// Lets the pointer rest where it is for long enough that a tooltip or a hover card shows, then draws.
    pub(crate) fn wait_out_a_tooltip(&mut self) {
        self.cx.executor().advance_clock(LONGER_THAN_A_TOOLTIP_WAITS);
        self.frame();
    }
}
