//! What a fake hub sends a client that follows through `Subscribe`, from what the world holds, as a hub sends it at
//! each level: where it stands first, and nothing after, since nothing here changes. And what clients follow now, for a
//! test to read.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;
use protocol::client::github::GithubUpdate;
use protocol::client::listed::{Listed, SessionRecords};
use protocol::client::subscriptions::{Follow, LogFrom, Refused, Subscribe, Subscribed, Subscription};
use protocol::client::{ApiError, ClientResponse, ErrorCode, HubFrame, ObservationNotice};
use protocol::model::{Level, Place, Rev, Seq};

use super::github;
use super::logs::{log_from, places_from};
use super::serve::Outcome;
use super::terminals;
use super::world::World;

/// What each subscription of each connection follows now, which a test shares before the hub is given the world, to
/// tell what a client follows and when.
#[derive(Clone, Default)]
pub struct Following(Arc<Mutex<HashMap<(u64, u64), Level>>>);

impl Following {
    /// Whether some subscription follows what `of` says.
    pub fn any(&self, of: impl Fn(&Level) -> bool) -> bool {
        self.0.lock().values().any(of)
    }

    /// How many subscriptions follow what `of` says.
    pub fn count(&self, of: impl Fn(&Level) -> bool) -> usize {
        self.0.lock().values().filter(|level| of(level)).count()
    }

    /// Lets go of what connection `connection` followed, as it ends.
    pub(super) fn ended(&self, connection: u64) {
        self.0.lock().retain(|(of, _), _| *of != connection);
    }
}

/// Stops each subscription `subscribe` stops, then starts each it starts, with where each stands, all after the reply,
/// as a hub can send them, refusing what the world doesn't have, as a hub does.
pub(super) fn subscribe(world: &World, connection: u64, subscribe: Subscribe) -> Outcome {
    let Subscribe { stop, start } = subscribe;
    let mut following = world.following.0.lock();
    for subscription_id in stop {
        following.remove(&(connection, subscription_id));
    }
    let (mut refused, mut frames) = (Vec::new(), Vec::new());
    for Subscription { subscription_id, follow } in start {
        match stands(world, subscription_id, &follow) {
            Ok(sent) => {
                following.insert((connection, subscription_id), follow.level());
                frames.extend(sent);
            }
            Err(error) => refused.push(Refused { subscription_id, error }),
        }
    }
    Outcome::Reply(ClientResponse::Subscribed(Subscribed { refused }), frames)
}

/// Where what `follow` follows stands, as the frames a hub sends first.
fn stands(world: &World, subscription_id: u64, follow: &Follow) -> Result<Vec<HubFrame>, ApiError> {
    match follow {
        Follow::Hub => Ok(hub(world, subscription_id)),
        Follow::Workspace { id } => {
            world.workspaces.iter().find(|workspace| workspace.id == *id).ok_or_else(|| not_found("workspace"))?;
            Ok(Vec::new())
        }
        Follow::Session { id, from } => {
            world.session(id)?;
            let records = SessionRecords {
                session_id: id.clone(),
                subagents: world.subagents_of(id),
                proposals: world.proposals.get(id).cloned().unwrap_or_default(),
                replies_heard: world.replies_heard_of(id)?,
                rev: Rev::default(),
            };
            let listed = HubFrame::Listed { subscription_id, listed: Listed::Session(records) };
            let log = match from {
                None => log_from(world, id, Seq::new(1), subscription_id),
                Some(LogFrom::Seq { seq }) => log_from(world, id, *seq, subscription_id),
                Some(LogFrom::Place { .. }) if world.reads_every_event_together => {
                    return Err(ApiError::new(ErrorCode::Unsupported, "a hub that can't read the agent's own apart"));
                }
                Some(LogFrom::Place { subagent, place }) => {
                    let subagent = world.subagent_named(id, subagent.as_ref());
                    places_from(world, id, subagent.as_ref(), (*place).max(Place::new(1)), subscription_id)
                }
                Some(LogFrom::Unknown) => return Err(ApiError::new(ErrorCode::Unsupported, "a place it doesn't know")),
            };
            Ok(std::iter::once(listed).chain(log).collect())
        }
        Follow::Terminal { id } => terminals::pictures(world, id, subscription_id),
        Follow::Reference { reference } => {
            let update =
                GithubUpdate::Reference { reference: reference.clone(), read: github::reference(world, reference) };
            Ok(vec![HubFrame::Github { subscription_id, update }])
        }
        Follow::Unknown => Err(ApiError::new(ErrorCode::Unsupported, "a level a fake hub doesn't know")),
    }
}

/// Every record and the hub's observations, then what the feed says of the world's observations, and what GitHub says
/// of every workspace's own pull request, as a hub sends them at its level.
fn hub(world: &World, subscription_id: u64) -> Vec<HubFrame> {
    let listed = HubFrame::Listed { subscription_id, listed: Listed::Hub(Box::new(world.records())) };
    let notice = |notice: &ObservationNotice| HubFrame::Notice { subscription_id, notice: notice.clone().into() };
    let pull_requests = github::own_pull_requests(world).map(|update| HubFrame::Github { subscription_id, update });
    std::iter::once(listed).chain(world.feed.iter().map(notice)).chain(pull_requests).collect()
}

fn not_found(what: &str) -> ApiError {
    ApiError::new(ErrorCode::NotFound, format!("a fake hub has no such {what}"))
}
