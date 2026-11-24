//! Which of the hub's lists of models the new-workspace dialog or a chat's picker follows. The hub holds each list and
//! announces each change of it, so this only asks which list applies, follows that one in the records, and asks again
//! when it's for somewhere else, when the hub has let go of it, or on Retry.

use client_core::Client;
use client_core::records::{ListOfModels, Snapshot};
use gpui_kit::{AnyElement, Context, IntoElement as _};
use protocol::client::admin::{ModelTarget, ModelsFor};
use protocol::model::{AgentDescription, ModelListKey, ModelLookup, Session};

/// The hub's list of models that a dialog or a chat's picker follows, and what it last asked for one.
#[derive(Default)]
pub struct FollowedModels {
    /// What was asked last, so the same isn't asked again while the hub holds its list.
    asked: Option<ModelsFor>,
    /// Which list the hub said applies, or what it said instead. The list itself is read from the records, which take
    /// in the reply's copy before the reply comes back.
    answer: Option<Result<ModelListKey, String>>,
    /// The records have held the list since it was answered, so its going is the hub letting go of it.
    seen: bool,
    /// Counts what's asked, so an answer to what was asked before isn't taken.
    generation: u64,
}

impl FollowedModels {
    /// Forgets what was asked, as on a new connection, whose hub may hold other lists.
    pub fn reset(&mut self) {
        self.generation += 1;
        (self.asked, self.answer, self.seen) = (None, None, false);
    }

    /// Follows the list for `agent` where `target` would launch, asking the hub when it's another than the one
    /// followed, when the hub has let go of that one, or `again`, as Retry does. `records` is the copy of the hub's
    /// records that holds the list followed. Nothing is asked for an agent that doesn't discover its models.
    #[expect(clippy::too_many_arguments, reason = "what's asked, where it's held, and where the answer goes")]
    pub fn follow<T: 'static>(
        &mut self,
        client: &Client,
        agent: Option<&AgentDescription>,
        target: ModelTarget,
        records: &Snapshot,
        again: bool,
        cx: &mut Context<T>,
        lookup: fn(&mut T) -> &mut Self,
    ) {
        let held = self.held(records);
        let Some(agent) = agent.filter(|agent| agent.discovers_models()) else {
            if self.asked.is_some() {
                self.reset();
            }
            return;
        };
        let request = ModelsFor { agent: agent.kind, target, again: false };
        self.seen |= held.is_some();
        let gone = self.seen && held.is_none();
        if self.asked.as_ref() == Some(&request) && !gone && !again {
            return;
        }
        self.reset();
        let generation = self.generation;
        self.asked = Some(request.clone());
        let reading = client.start(move |client| client.models_for(ModelsFor { again, ..request }));
        cx.spawn(async move |this, cx| {
            let answer = reading.recv().await.map(|list| list.key).map_err(|e| e.to_string());
            let _ = this.update(cx, |owner, cx| {
                let state = lookup(owner);
                if state.generation == generation {
                    state.answer = Some(answer);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Follows the list of models of the chat `session`, as [`FollowedModels::follow`] says, from what `records`
    /// hold. For whenever which chat shows changes, or what the hub holds does, such as the chat's login, or the hub
    /// letting go of the list.
    pub fn follow_chat<T: 'static>(
        &mut self,
        client: &Client,
        records: &Snapshot,
        session: &Session,
        again: bool,
        cx: &mut Context<T>,
        lookup: fn(&mut T) -> &mut Self,
    ) {
        let (agent, target) = (records.agents().of(session.agent), ModelTarget::Session(session.id.clone()));
        // A chat moved to another account launches under its login, which has a list of its own.
        if self.key().is_some_and(|key| session.account.is_some() && key.account != session.account) {
            self.reset();
        }
        self.follow(client, agent, target, records, again, cx, lookup);
    }

    /// What the chat `session`'s agent is offered where it runs, for its model picker, as
    /// [`FollowedModels::description`] says.
    pub fn chat_agent(&self, records: &Snapshot, session: &Session) -> Option<AgentDescription> {
        self.description(records.agents().of(session.agent)?, records)
    }

    /// What `records` hold of the list followed, the one place it's read.
    fn held<'a>(&self, records: &'a Snapshot) -> Option<ListOfModels<'a>> {
        records.list_of_models(self.key()?)
    }

    /// Which list is followed, once the hub has said.
    pub fn key(&self) -> Option<&ModelListKey> {
        self.answer.as_ref()?.as_ref().ok()
    }

    /// What `base` offers where the list followed applies: `base` itself for an agent that doesn't discover its models,
    /// and for one that does, `base` with the list's models, as `records` read them, once the agent has given some.
    pub fn description(&self, base: &AgentDescription, records: &Snapshot) -> Option<AgentDescription> {
        if !base.discovers_models() {
            return Some(base.clone());
        }
        let catalog = self.held(records)?.catalog.filter(|catalog| !catalog.offered.is_empty())?;
        Some(base.clone().with_catalog(catalog.clone()))
    }

    /// Whether the agent hasn't said yet what it offers, and hasn't failed to, so there's nothing to pick from.
    pub fn waiting(&self, records: &Snapshot) -> bool {
        let unlisted = self.held(records).is_none_or(|list| list.catalog.is_none());
        self.asked.is_some() && self.failed(records).is_none() && unlisted
    }

    /// What the hub said instead of a list, or why the list it holds has nothing to offer.
    fn failed<'a>(&'a self, records: &'a Snapshot) -> Option<&'a str> {
        let list = match self.answer.as_ref()? {
            Err(refused) => return Some(refused),
            Ok(_) => self.held(records)?,
        };
        match (list.catalog, list.lookup) {
            (Some(catalog), _) if !catalog.offered.is_empty() => None,
            (_, ModelLookup::Failed { message }) => Some(message),
            (Some(_), _) => Some("This login offers no models"),
            (None, _) => None,
        }
    }

    /// "Loading models…" while the agent is asked, and what went wrong, with Retry, once it fails, as `records` hold
    /// the list followed. `retry` asks again.
    pub fn status<T: 'static>(
        &self,
        records: &Snapshot,
        cx: &mut Context<T>,
        retry: fn(&mut T, &mut Context<T>),
    ) -> Option<AnyElement> {
        let error = self.failed(records);
        if error.is_none() && !self.waiting(records) {
            return None;
        }
        let label = error.map_or_else(|| "Loading models…".to_owned(), |e| format!("{e} · Retry"));
        let button = look::button("load-models", look::ButtonKind::Borderless, look::ControlSize::Small, label);
        Some(
            button
                .off(error.is_none())
                .on_click(cx.listener(move |owner, _, _, cx| retry(owner, cx)))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use client_core::records::HubRecords;
    use protocol::capability::settings::ModelCatalog;
    use protocol::model::{ModelList, Record, Rev};

    /// A client's copy of the hub's records holding `lists`.
    fn holding(lists: &[&ModelList]) -> Snapshot {
        let mut held = HubRecords::default();
        for list in lists {
            held.apply_record(Record::ModelList((*list).clone()));
        }
        held.snapshot()
    }

    fn list(rev: u64, lookup: ModelLookup, catalog: Option<ModelCatalog>) -> ModelList {
        let key = ModelListKey {
            agent: protocol::model::AgentKind::Codex,
            repo: "r".into(),
            provider: "local".into(),
            account: None,
            cli_version: None,
        };
        ModelList { key, catalog, lookup, rev: Rev::new(rev) }
    }

    /// The reply names the list, and what it says is read from the records alone, which take the reply's copy in
    /// before the reply comes back.
    #[test]
    fn the_list_answered_is_read_from_the_records() {
        let asked = ModelsFor {
            agent: protocol::model::AgentKind::Codex,
            target: ModelTarget::Project("r".into()),
            again: false,
        };
        let answer = Some(Ok(list(1, ModelLookup::Asking, None).key));
        let lookup = FollowedModels { asked: Some(asked), answer, ..FollowedModels::default() };
        let asking = list(2, ModelLookup::Asking, None);
        let failed = list(3, ModelLookup::Failed { message: "logged out".into() }, None);

        assert!(lookup.waiting(&holding(&[])) && lookup.waiting(&holding(&[&asking])), "nothing is listed yet");
        assert_eq!(lookup.failed(&holding(&[&failed])), Some("logged out"));
        assert!(!lookup.waiting(&holding(&[&failed])));
    }
}
