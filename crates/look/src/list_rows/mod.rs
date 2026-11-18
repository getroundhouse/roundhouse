//! A list's rows. A row stands for one of what a list holds, such as a file or a page in Settings, rounded and a little
//! in from its list's edge, and washed in the text's colour under the pointer and while it's chosen, as a borderless
//! button is, so every list lights the same way, and as a menu's rows do. `docs/spec/components.md` says how.

mod row;
pub use row::{ListRow, ROWS_INSET, list_row};
