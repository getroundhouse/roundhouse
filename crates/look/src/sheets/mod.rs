//! A handheld's sheet. It comes up from the bottom of the screen, over what it dims, with a few things to choose from
//! or fill in, where a thumb reaches them, and a tap outside it puts it away. An action sheet asks before something
//! that removes, in the same place. Only a handheld has one: at a desk the
//! same choices are a menu beside what opened it, or a dialog. `docs/spec/components.md` has its rules.

mod action_sheet;
mod dimmed;
mod sheet;
pub use action_sheet::{ActionSheet, action_sheet};
pub use sheet::{Sheet, sheet};
