//! Adding a project from the repositories the hub's GitHub accounts can see, or by an address on another host, with no
//! checkout on the hub's machine.

mod dialog;
pub(crate) use dialog::{Folder, open};
mod rows;
