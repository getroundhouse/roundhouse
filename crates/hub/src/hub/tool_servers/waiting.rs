use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;
use protocol::client::tool_sign_ins::SIGN_IN_WAITS;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::ToolSignInId;
use tokio::sync::Mutex as AsyncMutex;
use tool_sign_in::Pending;

use crate::store::later_ms;
use crate::tool_servers::SigningIn;

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
}

/// One sign-in that waits.
pub(super) struct WaitingSignIn {
    pub(super) to: SigningIn,
    /// Held while a finish is under way, so a finish sent again is answered with what the first one comes to.
    pub(super) stage: AsyncMutex<Stage>,
}

/// How far a sign-in has got.
pub(super) enum Stage {
    /// Its server hasn't been asked how to sign in to it, or hasn't answered.
    Finding,
    /// The person has a page to approve it on.
    OnPage { pending: Box<Pending> },
}

impl Waiting {
    /// The sign-in `id`, if it waits at `now_ms`.
    pub(super) fn get(&self, id: &ToolSignInId, now_ms: i64) -> Option<Arc<WaitingSignIn>> {
        let mut by_id = self.by_id.lock();
        forget_late(&mut by_id, now_ms);
        Some(Arc::clone(&by_id.get(id)?.sign_in))
    }

    /// Has the sign-in `id`, `to`, wait from `now_ms`. Refused with `Conflict` while one of that ID waits.
    pub(super) fn begin(&self, id: &ToolSignInId, to: SigningIn, now_ms: i64) -> Result<Arc<WaitingSignIn>, ApiError> {
        let mut by_id = self.by_id.lock();
        forget_late(&mut by_id, now_ms);
        if by_id.contains_key(id) {
            return Err(ApiError::new(ErrorCode::Conflict, "a sign-in to an MCP server with that ID is under way"));
        }
        let sign_in = Arc::new(WaitingSignIn { to, stage: AsyncMutex::new(Stage::Finding) });
        let entry = Entry { sign_in: Arc::clone(&sign_in), until_ms: later_ms(now_ms, SIGN_IN_WAITS) };
        by_id.insert(id.clone(), entry);
        Ok(sign_in)
    }

    /// Whether `sign_in` still waits as `id`.
    pub(super) fn holds(&self, id: &ToolSignInId, sign_in: &Arc<WaitingSignIn>) -> bool {
        self.by_id.lock().get(id).is_some_and(|entry| Arc::ptr_eq(&entry.sign_in, sign_in))
    }

    /// Forgets the sign-in `id`.
    pub(super) fn cancel(&self, id: &ToolSignInId) {
        self.by_id.lock().remove(id);
    }

    /// Forgets `sign_in`, which has ended.
    pub(super) fn end(&self, id: &ToolSignInId, sign_in: &Arc<WaitingSignIn>) {
        self.by_id.lock().retain(|held, entry| held != id || !Arc::ptr_eq(&entry.sign_in, sign_in));
    }
}

/// Forgets each that has waited as long as one does.
fn forget_late(by_id: &mut HashMap<ToolSignInId, Entry>, now_ms: i64) {
    by_id.retain(|_, entry| entry.until_ms > now_ms);
}
