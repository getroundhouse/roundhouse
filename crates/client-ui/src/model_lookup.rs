//! Which of the hub's lists of models the new-workspace dialog or a chat's picker follows. The hub holds each list and
//! announces each change of it, so this only asks which list applies, follows that one in the records, and asks again
//! when it's for somewhere else, when the hub has let go of it, or on Retry.

use client_core::Client;
use client_core::records::Snapshot;
use gpui_kit::{AnyElement, Context, IntoElement as _};
use protocol::client::admin::{ModelTarget, ModelsFor};
use protocol::model::{AgentDescription, ModelList, ModelListKey, ModelLookup, Session};

/// The hub's list of models that a dialog or a chat's picker follows, and what it last asked for one.
#[derive(Default)]
pub struct FollowedModels {
    /// What was asked last, so the same isn't asked again while the hub holds its list.
    asked: Option<ModelsFor>,
    /// The list the hub said applies, as its reply had it, or what it said instead.
    answer: Option<Result<ModelList, String>>,
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
    /// followed, when the hub has let go of that one, or `again`, as Retry does. `held` is what the records hold of the
    /// list followed. Nothing is asked for an agent that doesn't discover its models.
    #[expect(clippy::too_many_arguments, reason = "what's asked, where it's held, and where the answer goes")]
    pub fn follow<T: 'static>(
        &mut self,
        client: &Client,
        agent: Option<&AgentDescription>,
        target: ModelTarget,
        held: Option<&ModelList>,
        again: bool,
        cx: &mut Context<T>,
        lookup: fn(&mut T) -> &mut Self,
    ) {
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
            let answer = reading.recv().await.map_err(|e| e.to_string());
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
        let list = self.held(records);
        self.follow(client, agent, target, list, again, cx, lookup);
    }

    /// What the chat `session`'s agent is offered where it runs, for its model picker, as
    /// [`FollowedModels::description`] says.
    pub fn chat_agent(&self, records: &Snapshot, session: &Session) -> Option<AgentDescription> {
        self.description(records.agents().of(session.agent)?, self.held(records))
    }

    /// What `records` hold of the list followed.
    pub fn held<'a>(&self, records: &'a Snapshot) -> Option<&'a ModelList> {
        self.key().and_then(|key| records.model_list(key))
    }

    /// Which list is followed, once the hub has said.
    pub fn key(&self) -> Option<&ModelListKey> {
        Some(&self.answer.as_ref()?.as_ref().ok()?.key)
    }

    /// The newest of what the records hold of the list followed, `held`, and what the reply had.
    fn list<'a>(&'a self, held: Option<&'a ModelList>) -> Option<&'a ModelList> {
        let answered = self.answer.as_ref()?.as_ref().ok()?;
        Some(held.filter(|held| held.rev >= answered.rev).unwrap_or(answered))
    }

    /// What `base` offers where the list followed applies: `base` itself for an agent that doesn't discover its models,
    /// and for one that does, `base` with the list's models, once the agent has given some.
    pub fn description(&self, base: &AgentDescription, held: Option<&ModelList>) -> Option<AgentDescription> {
        if !base.discovers_models() {
            return Some(base.clone());
        }
        let catalog = self.list(held)?.catalog.clone().filter(|catalog| !catalog.offered.is_empty())?;
        Some(base.clone().with_catalog(catalog))
    }

    /// Whether the agent hasn't said yet what it offers, and hasn't failed to, so there's nothing to pick from.
    pub fn waiting(&self, held: Option<&ModelList>) -> bool {
        self.asked.is_some() && self.failed(held).is_none() && self.list(held).is_none_or(|list| list.catalog.is_none())
    }

    /// What the hub said instead of a list, or why the list it holds has nothing to offer.
    fn failed<'a>(&'a self, held: Option<&'a ModelList>) -> Option<&'a str> {
        match self.answer.as_ref()? {
            Err(refused) => Some(refused),
            Ok(_) => match self.list(held)? {
                ModelList { catalog: Some(catalog), .. } if !catalog.offered.is_empty() => None,
                ModelList { lookup: ModelLookup::Failed { message }, .. } => Some(message),
                ModelList { catalog: Some(_), .. } => Some("This login offers no models"),
                ModelList { catalog: None, .. } => None,
            },
        }
    }

    /// "Loading models…" while the agent is asked, and what went wrong, with Retry, once it fails. `retry` asks again.
    pub fn status<T: 'static>(
        &self,
        held: Option<&ModelList>,
        cx: &mut Context<T>,
        retry: fn(&mut T, &mut Context<T>),
    ) -> Option<AnyElement> {
        let error = self.failed(held);
        if error.is_none() && !self.waiting(held) {
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
    use protocol::capability::settings::ModelCatalog;
    use protocol::model::Rev;

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

    /// The reply can come before the feed's copy, and the feed's is newer once it does.
    #[test]
    fn the_newer_of_the_reply_and_the_records_copy_is_followed() {
        let asked = ModelsFor {
            agent: protocol::model::AgentKind::Codex,
            target: ModelTarget::Project("r".into()),
            again: false,
        };
        let answer = Some(Ok(list(2, ModelLookup::Asking, None)));
        let lookup = FollowedModels { asked: Some(asked), answer, ..FollowedModels::default() };
        let older = list(1, ModelLookup::Answered, Some(ModelCatalog::default()));
        let newer = list(3, ModelLookup::Failed { message: "logged out".into() }, None);

        assert!(lookup.waiting(None) && lookup.waiting(Some(&older)), "the reply is newer than what's held");
        assert_eq!(lookup.failed(Some(&newer)), Some("logged out"));
        assert!(!lookup.waiting(Some(&newer)));
    }
}
