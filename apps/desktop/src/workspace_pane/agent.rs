//! What the hub says of the agent each chat runs, from client-core's copy: what the model picker offers, what the
//! context's cards say and whether they offer to compact, what a turn's card names, and which kind a new chat runs.

use gpui_kit::{AnyElement, Context};
use protocol::model::{AgentDescription, AgentKind};

use super::WorkspacePane;

impl WorkspacePane {
    /// What the hub says of the agent the chat showing runs. `None` with no chat, and from a hub that doesn't describe
    /// it.
    pub(super) fn agent(&self) -> Option<AgentDescription> {
        let kind = self.session()?.agent;
        self.records.snapshot().agents().of(kind).cloned()
    }

    /// The same, with what it offers where the chat runs for one that discovers its models, for the model picker.
    /// `None` too until such an agent has said what it offers there, and once it has failed to.
    pub(super) fn picker_agent(&self) -> Option<AgentDescription> {
        self.model_lookup.chat_agent(&self.records.snapshot(), self.session()?)
    }

    /// Follows the list of models of the chat showing, as `Lookup::follow` says. For whenever which chat shows changes,
    /// or what the hub holds does, such as the chat's login, or the hub letting go of the list.
    pub(super) fn follow_models(&mut self, again: bool, cx: &mut Context<Self>) {
        let Some(session) = self.session().cloned() else { return };
        let (client, records) = (self.client.clone(), self.records.snapshot());
        self.model_lookup.follow_chat(&client, &records, &session, again, cx, |p| &mut p.model_lookup);
    }

    /// Whether there's nothing to pick from yet, since the agent hasn't said what it offers.
    pub(super) fn models_waiting(&self) -> bool {
        self.model_lookup.waiting(self.model_lookup.held(&self.records.snapshot()))
    }

    /// "Loading models…", or what went wrong, with Retry.
    pub(super) fn models_status(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.model_lookup
            .status(self.model_lookup.held(&self.records.snapshot()), cx, |p, cx| p.follow_models(true, cx))
    }

    /// Whether the agent of the chat showing compacts when asked, so `Compact now` and a typed `/compact` ask it to.
    /// One the hub doesn't describe isn't asked, since it may not.
    pub(super) fn compacts(&self) -> bool {
        self.session().and_then(|s| s.agent_capabilities.as_ref()).map_or_else(
            || self.agent().is_some_and(|agent| agent.compacts()),
            |run| run.claims.context.compaction.is_some(),
        )
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
