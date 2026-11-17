//! Dialogs: what's asked or shown over the window, which dims the rest until it's answered or closed, with its title,
//! what it says, what's in it and its answers, so every dialog is laid out the same. `docs/spec/components.md` has the
//! table of its widths, which a test holds to these.

#[cfg(test)]
mod tests;

mod dialog;
mod plain;
mod widths;
#[cfg(any(test, feature = "testing"))]
pub use dialog::shown_dialog;
pub use dialog::{Dialog, close_dialog, close_every_dialog, dialog, dialog_open, open_dialog};
pub use plain::{PlainDialog, open_plain_dialog, plain_dialog};
pub use widths::DialogWidth;
