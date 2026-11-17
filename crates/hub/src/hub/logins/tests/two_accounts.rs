//! Chats on a hub whose workspaces have stand-ins for their workers, on machines where nobody is logged in to Claude's
//! CLI unless a test says so, and what the hub hands each launch of their agents and records of which account each is
//! on.

use std::sync::Arc;

use protocol::client::HubNotice;
use protocol::client::accounts::AccountConnect;
use protocol::client::sessions::SessionAssignAccount;
use protocol::event::{EventBody, ExitReason, SessionEvent};
use protocol::model::{
    Account, AccountId, AgentKind, CacheRequest, CacheTtl, LoginToken, Seq, Session, SessionId, SessionSettingsChange,
    Workspace,
};
use protocol::prompt::Prompt;
use tokio::sync::broadcast;

use crate::hub::Hub;
use crate::hub::stand_in_workers::{Asked, TIMEOUT, a_chat, a_turn, hub_with_stand_in_workers, the_agent_says};

/// A hub whose workspaces have stand-ins for their workers, and what those have been asked.
pub(super) struct Chats {
    pub(super) hub: Arc<Hub>,
    pub(super) asked: Arc<Asked>,
}

impl Chats {
    pub(super) fn new(dir: &std::path::Path) -> Self {
        let (hub, asked) = hub_with_stand_in_workers(dir);
        Self { hub, asked }
    }

    /// Has the hub hold a login for Claude called `name`, whose token is its name in lower case, then `-token`.
    pub(super) async fn connect(&self, name: &str) -> Account {
        let token = LoginToken::new(format!("{}-token", name.to_lowercase()));
        let asked = AccountConnect { agent: AgentKind::Claude, name: name.to_string(), token, account: None };
        self.hub.account_connect(asked).await.unwrap()
    }

    /// Another chat of Claude's in `workspace`, on no account until its agent first launches.
    pub(super) async fn another_chat(&self, workspace: &Workspace) -> SessionId {
        let unchanged = SessionSettingsChange::default();
        self.hub.session_create(workspace.id.clone(), AgentKind::Claude, unchanged, None).await.unwrap().id
    }

    /// A turn of the chat's agent, which first asks its model `asking`, if anything, and the token of the login its
    /// launch was handed, or `None` when it was handed none and runs under its machine's own.
    pub(super) async fn token_in_turn(&self, chat: &SessionId, asking: Option<CacheRequest>) -> Option<String> {
        let spec = a_turn(&self.hub, &self.asked, chat, Prompt::new("go"), asking).await;
        spec.login.map(|login| login.token.as_str().to_string())
    }

    /// The agent exits by itself between turns, so the next prompt launches another under whatever account the chat
    /// runs under by then.
    pub(super) async fn crash(&self, chat: &SessionId) {
        let exited =
            EventBody::AgentExited { reason: ExitReason::Unexpected, code: Some(3), signal: None, stderr_tail: vec![] };
        the_agent_says(&self.hub, &self.asked, chat, [exited]).await;
    }

    /// Prompts a chat whose launch is refused, and says why, as the launch's failure does.
    pub(super) async fn refused_launch(&self, chat: &SessionId) -> String {
        self.hub.session_prompt(chat.clone(), None, Prompt::new("go")).await.unwrap();
        let failed = || {
            let failed = self.log(chat).into_iter().find_map(|e| {
                let EventBody::AgentExited { reason: ExitReason::StartFailed, stderr_tail, .. } = e.body else {
                    return None;
                };
                Some(stderr_tail)
            });
            failed.ok_or("the launch didn't fail")
        };
        waiting::task::until_ok(TIMEOUT, failed).await.join("\n")
    }

    pub(super) async fn assign(
        &self,
        chat: &SessionId,
        account: &AccountId,
        force: bool,
    ) -> Result<Session, protocol::client::ApiError> {
        let asked = SessionAssignAccount { id: chat.clone(), account: account.clone(), force };
        self.hub.session_assign_account(asked).await
    }

    /// The account the chat is on, as the hub records it.
    pub(super) fn on(&self, chat: &SessionId) -> Option<AccountId> {
        self.record(chat).hub_account().cloned()
    }

    pub(super) fn record(&self, chat: &SessionId) -> Session {
        self.hub.store.get_session(chat).unwrap().session
    }

    pub(super) fn log(&self, chat: &SessionId) -> Vec<SessionEvent> {
        self.hub.store.read_events(chat, Seq::FIRST, 1000, u64::MAX, false).unwrap()
    }

    /// Whether each launch of the chat's agent resumed its conversation, in order, as the log records them.
    pub(super) fn launches(&self, chat: &SessionId) -> Vec<bool> {
        let resumed = |e: SessionEvent| {
            let EventBody::AgentStarted { resumed, .. } = e.body else { return None };
            Some(resumed)
        };
        self.log(chat).into_iter().filter_map(resumed).collect()
    }

    /// Each time the log says one of the chat's requests found the cache expired.
    pub(super) fn found_expired(&self, chat: &SessionId) -> Vec<EventBody> {
        let expired = |e: SessionEvent| matches!(e.body, EventBody::CacheExpired { .. }).then_some(e.body);
        self.log(chat).into_iter().filter_map(expired).collect()
    }
}

/// A request that reads nothing of the cache and writes the conversation into it, as the first for a prompt, as one
/// does when nothing of the conversation is cached, and as one does once what was cached has expired.
pub(super) fn cold() -> CacheRequest {
    CacheRequest {
        sent_ms_ago: 0,
        ttl: Some(CacheTtl::FiveMinutes),
        read_tokens: 0,
        written_tokens: 20_000,
        model: None,
        first_for_prompt: true,
    }
}

/// A hub that holds logins for two accounts of Claude's, Work, which is the default as the first added, and Personal,
/// whose workspaces' machines have nobody logged in, so every launch runs under a login the hub hands out, and a chat
/// that has had a turn under Work, which put it on Work. Its agent wrote the conversation into the cache in that turn,
/// so the chat's record says what the cache holds.
pub(super) struct TwoAccounts {
    pub(super) chats: Chats,
    pub(super) workspace: Workspace,
    pub(super) work: Account,
    pub(super) personal: Account,
    pub(super) chat: SessionId,
}

impl TwoAccounts {
    pub(super) async fn start(dir: &std::path::Path) -> Self {
        let chats = Chats::new(dir);
        let (work, personal) = (chats.connect("Work").await, chats.connect("Personal").await);
        let (workspace, chat) = a_chat(&chats.hub).await;
        assert_eq!(chats.token_in_turn(&chat.id, Some(cold())).await.as_deref(), Some("work-token"));
        assert!(chats.record(&chat.id).prompt_cache.is_some(), "its first turn left something cached");
        Self { chats, workspace, work, personal, chat: chat.id }
    }

    pub(super) async fn another_chat(&self) -> SessionId {
        self.chats.another_chat(&self.workspace).await
    }
}

/// The account the feed first announces `chat` as on.
pub(super) async fn announced_on(feed: &mut broadcast::Receiver<HubNotice>, chat: &SessionId) -> AccountId {
    loop {
        let told = tokio::time::timeout(TIMEOUT, feed.recv()).await.expect("the chat to be announced as on an account");
        if let HubNotice::Session(told) = told.expect("the feed goes on")
            && told.id == *chat
            && let Some(account) = told.hub_account()
        {
            return account.clone();
        }
    }
}
