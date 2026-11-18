//! Tabs. A tab names one of what a bar of them can show, such as a chat in the workspace's row of tabs or the
//! sidebar's Changes, underlined while it's showing. `docs/spec/components.md` has the table of a tab's widths, which a
//! test holds to these.

#[cfg(test)]
mod tests;

mod tab;
mod widths;
pub use tab::{Tab, tab};
pub use widths::TabWidth;
