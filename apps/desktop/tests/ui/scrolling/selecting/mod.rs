//! Selecting what's written in a chat: by dragging, past the window's edge too, which scrolls the transcript on for as
//! long as the pointer is held out there, either way, and keeps what's selected, and over a link, which a drag doesn't
//! open; a word or a paragraph at a time; or all of it; and with the keys, which move an end.

use crate::tools::COMMAND_LINE;

mod areas;
mod end;
mod keys;
mod kinds;
mod made_up;
mod tab;
mod views;
pub(super) use views::{
    PARAGRAPHS, last_shown, on_the_first, painted, paragraphs, with_a_file, with_a_proposal, with_a_proposal_asking,
};

mod dragging;
use dragging::{
    LAST, PRINTED, bounds, everything, head, on_the_end_of_the_chat, on_the_reply_saying, open, settled, shift_click,
    tail,
};
pub(super) use dragging::{click, copy};
