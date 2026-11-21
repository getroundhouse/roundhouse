//! A handheld's bar across the top of a screen. It names the screen in its middle, with what goes back at its start
//! and what's done there at its end, as a phone's own apps do, so a screen that fills the device still says where it
//! is and how to leave. Only a handheld has one: the desktop names what's showing in the strip along its window's
//! top. `docs/spec/components.md` has its rules.

#[cfg(test)]
mod tests;

mod top_bar;
pub use top_bar::{TopBar, top_bar};
