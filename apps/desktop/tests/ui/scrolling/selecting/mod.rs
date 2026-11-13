//! Selecting what's written in a chat: by dragging, past the window's edge too, which scrolls the transcript on for as
//! long as the pointer is held out there, either way, and keeps what's selected, and over a link, which a drag doesn't
//! open; a word or a paragraph at a time; or all of it.

use crate::tools::COMMAND_LINE;

mod calls;
mod end;
mod kinds;
mod tab;

mod dragging;
use dragging::{
    LAST, PRINTED, bounds, click, copy, everything, head, on_the_end_of_the_chat, on_the_reply_saying, open, settled,
    shift_click, tail,
};
