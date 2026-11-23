//! Following a session on hubs scripted for what a real hub can't be made to do on cue.

mod compactions;
mod prompts;
mod versions;
mod views;
mod waiting;

mod loading;
pub(super) use loading::{LAST_PLACE, LAST_SEQ, answered_simply, answers, requests_until_unsubscribed, session, until};
pub(in crate::client) use loading::{follows_session, refused, refusing_every_start, start_hub_answering};
