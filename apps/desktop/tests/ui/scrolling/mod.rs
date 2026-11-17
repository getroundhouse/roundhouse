//! Scrolling through a session longer than a transcript loads at once: by the wheel and by the scrollbar's thumb,
//! what the scrollbar says along the way, and how little of the session is measured for it.

#![cfg(test)]

mod cost;
mod late_heights;
mod latest;
mod selecting;

mod history;
use history::{A_FRAME, press, settle_at_the_end, thumb, wait_for_the_list_to_settle, wait_for_the_tail};
mod long_sessions;
use long_sessions::{LONG, open_on_a_long_chat, open_on_a_long_session, prompts, session_of, session_saying};
