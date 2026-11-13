//! Following a session on hubs scripted for what a real hub can't be made to do on cue.

mod compactions;
mod prompts;
mod views;
mod waiting;

mod loading;
pub(super) use loading::{LAST_PLACE, LAST_SEQ, answered_simply, requests_until_unsubscribed, session, until};
pub(in crate::client) use loading::{refused, start_hub_answering};
