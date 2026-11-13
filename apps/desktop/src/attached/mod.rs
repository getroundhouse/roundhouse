//! Files waiting to go with a prompt that hasn't been sent, in a chat's composer or the new-workspace dialog: the list
//! of them, drawn as tiles above what's typed, and the file picker and drop target that add to it. Each file is read
//! and checked as it comes, in `reading`, so nothing that would be refused gets as far as sending. A prompt that can't
//! be sent comes back with its files. The hub keeps a chat's with its draft, and the dialog's with what's in it, so each
//! says where it was given to.

mod from_the_hub;
mod reading;

pub(crate) use from_the_hub::read_back;
pub(crate) use reading::{pasted, pick, read_into};

mod attachments;
pub(crate) use attachments::{Attached, Attachments, joined, read_paths_into, takes_drops};
