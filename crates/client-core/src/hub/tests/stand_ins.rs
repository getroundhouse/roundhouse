//! The stand-in hubs and the link's tries the `Hub`'s tests share.

use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use protocol::client::listed::Listed;
use protocol::client::subscriptions::{Follow, Subscribe};
use protocol::client::{ApiError, ErrorCode, HubFrame, ReplyOutcome};

use crate::link::{Connect, Lost, Refused};
use crate::stand_in_hub::{self, Does};
use crate::{ClientError, Connection};

pub(super) const TIMEOUT: Duration = Duration::from_secs(10);
/// Longer than any test runs, so the link tries again only when it's asked to.
pub(super) const NEVER: Duration = Duration::from_secs(600);

/// Each try gets the next of these, in turn, and fails as a hub that can't be reached does once they've all been tried.
#[derive(Clone)]
pub(super) struct Tries(Arc<Mutex<Vec<Result<Connection, ClientError>>>>);

impl Tries {
    pub(super) fn of(each: Vec<Result<Connection, ClientError>>) -> Tries {
        Tries(Arc::new(Mutex::new(each.into_iter().rev().collect())))
    }
}

impl Connect for Tries {
    type With = ();
    type Failure = String;

    fn connect(&mut self) -> Result<(Connection, ()), Refused<String>> {
        match self.0.lock().pop().unwrap_or(Err(ClientError::Disconnected)) {
            Ok(client) => Ok((client, ())),
            Err(error) => Err(Refused::by(&error, error.to_string())),
        }
    }

    fn lost(&self, lost: &Lost) -> String {
        lost.to_string()
    }
}

/// What a hub does with `subscribe`: lists its records where it starts following the hub, and sends nothing of the
/// rest, such as a terminal.
pub(super) fn listing(subscribe: &Subscribe) -> Does {
    stand_in_hub::subscribed(subscribe, |subscription_id, follow| {
        let listed = (*follow == Follow::Hub).then(|| Listed::Hub(Box::default()));
        Vec::from_iter(listed.map(|listed| HubFrame::Listed { subscription_id, listed }))
    })
}

/// What a hub does with what a test isn't about, such as the records asking which agents it offers.
pub(super) fn doesnt_know() -> Does {
    Does::Answers(Box::new(ReplyOutcome::Error(ApiError::new(ErrorCode::Unsupported, "refused"))))
}
