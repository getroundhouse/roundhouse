use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use protocol::client::tool_sign_ins::ToolSignInPage;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::ToolSignInId;
use tokio::sync::Mutex as AsyncMutex;
use tool_sign_in::Pending;

use crate::store::later_ms;
use crate::tool_servers::SigningIn;

/// How long a sign-in waits, from when it's started, which is as long as GitHub's codes last, and enough for someone to
/// find a password first. *Unconfirmed.*
pub(super) const WAITS: Duration = Duration::from_mins(15);

/// How many sign-ins can wait at once. One person signs in to one server at a time, so a few let others beside them, and
/// keep a client that starts them without end from having the hub hold many. *Unconfirmed.*
pub(super) const MOST_WAITING: usize = 4;

/// The sign-ins to tool servers that wait for the person, or for what the hub asks of them, by ID. Only the client that
/// started one can have it finished, since only it hears where the person is sent back, so no client is told of one, and
/// they're in memory, where a hub that stops loses them.
#[derive(Default)]
pub(super) struct Waiting {
    by_id: Mutex<HashMap<ToolSignInId, Entry>>,
}

struct Entry {
    sign_in: Arc<WaitingSignIn>,
    until_ms: i64,
    /// Whether what the person signed in is being kept, from when it's too late to cancel it, or to forget it.
    keeping: bool,
}

/// One sign-in that waits.
pub(super) struct WaitingSignIn {
    pub(super) to: SigningIn,
    /// Held while the servers are asked about it, so a start sent again is answered with what the first one finds, and a
    /// finish sent again with what the first one comes to.
    pub(super) stage: AsyncMutex<Stage>,
}

/// How far a sign-in has got.
pub(super) enum Stage {
    /// Its server hasn't been asked how to sign in to it, or hasn't answered.
    Finding,
    /// The person has a page to approve it on.
    OnPage { page: ToolSignInPage, pending: Box<Pending> },
}

impl Waiting {
    /// The sign-in `id`, if it waits at `now_ms`.
    pub(super) fn get(&self, id: &ToolSignInId, now_ms: i64) -> Option<Arc<WaitingSignIn>> {
        let mut by_id = self.by_id.lock();
        forget_late(&mut by_id, now_ms);
        Some(Arc::clone(&by_id.get(id)?.sign_in))
    }

    /// Has the sign-in `id`, `to`, wait from `now_ms`, or answers with the one of that ID that waits. Refused with
    /// `Unavailable` when as many wait as may.
    pub(super) fn begin(&self, id: &ToolSignInId, to: SigningIn, now_ms: i64) -> Result<Arc<WaitingSignIn>, ApiError> {
        let mut by_id = self.by_id.lock();
        forget_late(&mut by_id, now_ms);
        if let Some(entry) = by_id.get(id) {
            return Ok(Arc::clone(&entry.sign_in));
        }
        if by_id.len() >= MOST_WAITING {
            let message = "as many sign-ins to MCP servers as may wait at once are waiting; finish or cancel one";
            return Err(ApiError::new(ErrorCode::Unavailable, message));
        }
        let sign_in = Arc::new(WaitingSignIn { to, stage: AsyncMutex::new(Stage::Finding) });
        let entry = Entry { sign_in: Arc::clone(&sign_in), until_ms: later_ms(now_ms, WAITS), keeping: false };
        by_id.insert(id.clone(), entry);
        Ok(sign_in)
    }

    /// Whether `sign_in` still waits as `id`.
    pub(super) fn holds(&self, id: &ToolSignInId, sign_in: &Arc<WaitingSignIn>) -> bool {
        self.by_id.lock().get(id).is_some_and(|entry| Arc::ptr_eq(&entry.sign_in, sign_in))
    }

    /// Keeps `sign_in` from being cancelled or forgotten from now on, unless it has been.
    pub(super) fn keep(&self, id: &ToolSignInId, sign_in: &Arc<WaitingSignIn>) -> bool {
        let mut by_id = self.by_id.lock();
        let entry = by_id.get_mut(id).filter(|entry| Arc::ptr_eq(&entry.sign_in, sign_in));
        entry.map(|entry| entry.keeping = true).is_some()
    }

    /// Forgets the sign-in `id`, unless what it came to is being kept.
    pub(super) fn cancel(&self, id: &ToolSignInId) {
        self.by_id.lock().retain(|held, entry| held != id || entry.keeping);
    }

    /// Forgets `sign_in`, which has ended.
    pub(super) fn end(&self, id: &ToolSignInId, sign_in: &Arc<WaitingSignIn>) {
        self.by_id.lock().retain(|held, entry| held != id || !Arc::ptr_eq(&entry.sign_in, sign_in));
    }
}

/// Forgets each that has waited as long as one does.
fn forget_late(by_id: &mut HashMap<ToolSignInId, Entry>, now_ms: i64) {
    by_id.retain(|_, entry| entry.keeping || entry.until_ms > now_ms);
}
