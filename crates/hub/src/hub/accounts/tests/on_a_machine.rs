//! Chats whose workspaces' workers are stand-ins on machines where a test says who's logged in to Claude's CLI, and
//! whose agents hear of their account's limits as a test says.

use std::sync::Arc;

use parking_lot::Mutex;
use protocol::capability::Report;
use protocol::capability::account::Limit;
use protocol::client::ApiError;
use protocol::model::{AccountId, AgentAccount, LimitWindow, SessionId, UsageLimit, WorkspaceId};
use protocol::prompt::Prompt;

use crate::hub::Hub;
use crate::hub::observations::{AccountIdentity, AccountLimits};
use crate::hub::stand_in_workers::{Asked, TIMEOUT, a_turn, logged_in, the_agent_reports};

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
        let limits = limits.iter().map(Limit::of).collect();
        the_agent_reports(hub, asked, chat, Report::Limits { limits, every_models_week: false }).await;
    }
}

/// An account the hub has a record of, and what it observes of it.
#[derive(Debug)]
pub(super) struct Observed {
    pub(super) id: AccountId,
    /// Who it is, as a worker last read it, or none, while that isn't known.
    pub(super) identity: protocol::model::AccountIdentity,
    /// What's left of its limits, each as last heard, or none, while no agent under it has heard of one.
    pub(super) limits: Vec<UsageLimit>,
}

/// What the hub observes of the account `id`.
pub(super) fn observed(hub: &Hub, id: &AccountId) -> Observed {
    let identity = hub.observations.value_of::<AccountIdentity>(id).unwrap_or_default();
    let limits = hub.observations.value_of::<AccountLimits>(id).unwrap_or_default();
    Observed { id: id.clone(), identity, limits }
}

/// What the hub observes of the account the chat names, once it names one with a record that `holds` is true of.
pub(super) async fn named(hub: &Hub, chat: &SessionId, holds: impl Fn(&Observed) -> bool) -> Observed {
    let named = || {
        let named = hub.store.get_session(chat).unwrap().session.account;
        let recorded = named.as_ref().filter(|id| hub.store.account(id).unwrap().is_some());
        let account = recorded.map(|id| observed(hub, id));
        account.filter(&holds).ok_or_else(|| format!("the chat names {named:?}"))
    };
    waiting::task::until_ok(TIMEOUT, named).await
}
