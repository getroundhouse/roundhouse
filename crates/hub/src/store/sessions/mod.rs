//! Sessions.

#[cfg(test)]
use super::{Store, StoreError, lock};
#[cfg(test)]
use protocol::model::{QueuedPrompt, RunId, SessionStatus};

#[cfg(test)]
mod tests;

mod conversations;
mod session_login;
pub(crate) use session_login::SessionLogin;
pub(super) use session_login::{on_the_hubs, session_login_of};
mod session_rows;
pub(super) use session_rows::{SessionColumns, draft_column, forget_cache_unless_under, session_from_row};
