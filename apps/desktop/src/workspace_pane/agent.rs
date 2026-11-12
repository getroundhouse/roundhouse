//! What the hub says of the agent each chat runs, from client-core's copy: what the model picker offers, what the
//! context's cards say and whether they offer to compact, what a turn's card names, and which kind a new chat runs.

use gpui_kit::Context;
use protocol::model::{AgentDescription, AgentFeature, AgentKind};

use super::WorkspacePane;

impl WorkspacePane {
    /// What the hub says of the agent the chat showing runs. `None` with no chat, and from a hub that doesn't describe
    /// it.
    pub(super) fn agent(&self) -> Option<AgentDescription> {
        let kind = self.session()?.agent;
        self.records.snapshot().agents().of(kind).cloned()
    }

    /// Whether the agent of the chat showing compacts when asked, so `Compact now` and a typed `/compact` ask it to.
    /// One the hub doesn't describe isn't asked, since it may not.
    pub(super) fn compacts(&self) -> bool {
        self.agent().is_some_and(|agent| agent.has(AgentFeature::Compacts))
    }

    /// The kind of agent a new chat runs, as client-core says.
    pub(super) fn new_chat_agent(&self) -> AgentKind {
        self.records.snapshot().agents().for_new_chat()
    }

    /// Tells each chat's transcript what the hub says of its agent, which changes only with a new connection.
    pub(super) fn pass_on_agents(&self, cx: &mut Context<Self>) {
        let held = self.records.snapshot();
        for session in self.sessions() {
            if let Some((transcript, _)) = self.chats.get(&session.id).and_then(|chat| chat.transcript.as_ref()) {
                let agent = held.agents().of(session.agent);
                transcript.update(cx, |t, cx| t.set_agent(agent, cx));
            }
        }
    }
}
