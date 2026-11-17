//! Chats whose workspaces' workers are stand-ins on machines where a test says who's logged in to Claude's CLI, and
//! whose agents hear of their account's limits as a test says.

use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use protocol::client::ApiError;
use protocol::event::EventBody;
use protocol::model::{Account, AgentAccount, LimitWindow, SessionId, UsageLimit, WorkspaceId};
use protocol::prompt::Prompt;

use crate::hub::Hub;
use crate::hub::stand_in_workers::{Asked, a_turn, logged_in, the_agent_says};

/// How long the hub has to learn which account a launch runs under, and to keep what its agent heard, which it does on
/// tasks of its own. Far longer than that takes with a stand-in for the worker, so only what never comes runs out of it.
pub(super) const TIMEOUT: Duration = Duration::from_secs(10);

/// Who a machine's worker says is logged in to Claude's CLI there, or why it can't say, which a test can change.
pub(super) type Machine = Arc<Mutex<Result<AgentAccount, ApiError>>>;

/// Has the worker of `workspace` be a stand-in on a machine where `who` is logged in, until the test says otherwise.
pub(super) fn on_a_machine(hub: &Hub, workspace: &WorkspaceId, asked: &Arc<Asked>, who: AgentAccount) -> Machine {
    let machine = Arc::new(Mutex::new(Ok(who)));
    let reads = machine.clone();
    logged_in(hub, workspace, asked, move |_| reads.lock().clone());
    machine
}

/// The week's limit an agent hears of.
pub(super) fn the_week() -> UsageLimit {
    UsageLimit {
        window: LimitWindow::SevenDays,
        model: None,
        used_percent: 4,
        resets_at_ms: Some(4_102_444_800_000),
        heard_at_ms: None,
    }
}

/// The limits, without when each was heard.
pub(super) fn without_when(limits: &[UsageLimit]) -> Vec<UsageLimit> {
    limits.iter().map(|limit| UsageLimit { heard_at_ms: None, ..limit.clone() }).collect()
}

/// A turn of the chat's agent, in which it hears of `limits` by itself, as Claude's CLI does in its first turn, if
/// there are any.
pub(super) async fn a_turn_hearing(hub: &Arc<Hub>, asked: &Asked, chat: &SessionId, limits: Vec<UsageLimit>) {
    a_turn(hub, asked, chat, Prompt::new("go"), None).await;
    if !limits.is_empty() {
        the_agent_says(hub, asked, chat, [EventBody::AgentLimits { limits }]).await;
    }
}

/// The record of the account the chat names, once it names one that `holds` is true of.
pub(super) async fn named(hub: &Hub, chat: &SessionId, holds: impl Fn(&Account) -> bool) -> Account {
    let named = || {
        let named = hub.store.get_session(chat).unwrap().session.account;
        let account = named.as_ref().and_then(|id| hub.store.account(id).unwrap());
        account.filter(&holds).ok_or_else(|| format!("the chat names {named:?}"))
    };
    waiting::task::until_ok(TIMEOUT, named).await
}
