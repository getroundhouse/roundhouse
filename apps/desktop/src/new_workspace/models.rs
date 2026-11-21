//! The first chat's agent and its selected model and effort.

use super::NewWorkspaceDialog;
use client_ui::Picked;
use gpui_kit::{AnyElement, Context};
use protocol::capability::settings::Effort;
use protocol::client::admin::ModelTarget;
use protocol::model::{AgentDescription, AgentKind, ModelList, ModelName};

impl NewWorkspaceDialog {
    /// What the hub says of the agent the first chat runs, with what it offers in the dialog's project for one that
    /// discovers its models. `None` from a hub that doesn't describe it, and until the agent has said what it offers.
    pub(super) fn agent(&self) -> Option<AgentDescription> {
        self.model_lookup.description(self.base_agent()?, self.held_models())
    }

    /// What the hub holds of the list of models the dialog follows.
    fn held_models(&self) -> Option<&ModelList> {
        let key = self.model_lookup.key()?;
        self.offer.model_lists.iter().find(|list| list.key == *key)
    }

    /// Follows the list of models of the first chat's agent in the dialog's project, as `Lookup::follow` says. For
    /// whenever what it's for changes, or the hub's lists do.
    pub(crate) fn follow_models(&mut self, again: bool, cx: &mut Context<Self>) {
        let Some(client) = self.keeping.client().cloned() else { return };
        let (agent, held) = (self.base_agent().cloned(), self.held_models().cloned());
        let target = ModelTarget::Project(self.repo.clone());
        self.model_lookup.follow(&client, agent.as_ref(), target, held.as_ref(), again, cx, |d| &mut d.model_lookup);
    }

    /// Whether there's nothing to pick from yet, since the agent hasn't said what it offers.
    pub(super) fn models_waiting(&self) -> bool {
        self.model_lookup.waiting(self.held_models())
    }

    /// "Loading models…", or what went wrong, with Retry.
    pub(super) fn models_status(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.model_lookup.status(self.held_models(), cx, |d, cx| d.follow_models(true, cx))
    }

    pub(super) fn base_agent(&self) -> Option<&AgentDescription> {
        self.chosen_agent.and_then(|kind| self.offer.agents.of(kind)).or_else(|| self.offer.agents.new_chat())
    }

    pub(super) fn pick_agent(&mut self, agent: AgentKind, cx: &mut Context<Self>) {
        if self.offer.agents.of(agent).is_none() {
            return;
        }
        self.chosen_agent = Some(agent);
        self.model = None;
        self.effort = None;
        self.plan(self.planning());
        self.follow_models(false, cx);
        self.keep(true, cx);
        cx.notify();
    }

    /// The model and effort the first chat is to run: what was picked or kept, or else what its agent starts a chat
    /// with. Either is `None` from a hub that doesn't say, until it's picked or kept, which leaves it to the hub.
    pub(super) fn to_run(&self) -> (Option<ModelName>, Option<Effort>) {
        let agent = self.agent();
        let model = self.model.clone().or_else(|| agent.as_ref().and_then(|a| a.defaults().model));
        let effort = self
            .effort
            .clone()
            .or_else(|| agent.as_ref().zip(model.as_ref()).map(|(a, m)| a.effort_for_choice(m, None)));
        (model, effort)
    }

    pub(super) fn selection_available(&self) -> bool {
        self.agent().zip(self.shown()).is_some_and(|(agent, (model, effort))| {
            agent.label(&model).is_some() && agent.offers_effort(&model, &effort)
        })
    }

    /// The same, when both are known, for the picker to show.
    pub(super) fn shown(&self) -> Option<(ModelName, Effort)> {
        let (model, effort) = self.to_run();
        Some((model?, effort?))
    }

    /// Takes what was picked in the model picker. Closing the menu gives focus back to the prompt, so typing carries
    /// on there.
    pub(super) fn pick(&mut self, picked: Picked, cx: &mut Context<Self>) {
        match picked {
            Picked::Model(model) => {
                let (_, previous) = self.to_run();
                self.effort = self.agent().map(|agent| agent.effort_for_choice(&model, previous.as_ref()));
                self.model = Some(model);
            }
            Picked::Effort(effort) => self.effort = Some(effort),
        }
        self.keep(true, cx);
        cx.notify();
    }
}
