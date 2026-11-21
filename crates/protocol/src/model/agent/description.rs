use super::super::{AgentKind, ModelName};
use crate::capability::Capabilities;
use crate::capability::approvals::ApprovalMode;
use crate::capability::settings::{Effort, EffortOffer, ModelCatalog, ModelOffer, Models, Settings};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RunCapabilities {
    pub run: super::super::RunId,
    pub claims: Box<crate::capability::Capabilities>,
}

impl RunCapabilities {
    pub fn new(run: super::super::RunId, claims: crate::capability::Capabilities) -> Self {
        Self { run, claims: Box::new(claims) }
    }
}

/// A kind of agent, as its adapter describes it: what the interface calls it, what login a hub can hold for it, and
/// what a run of it claims, which say what a chat can be given and what it runs with until another is chosen. The hub
/// serves one for each kind it runs, so a client offers what the agent takes rather than holding a list of its own.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentDescription {
    pub kind: AgentKind,
    /// What a run of this kind claims unless it says otherwise as it launches.
    pub capabilities: Capabilities,
    /// What answered a turn, as its card says, such as "Opus 5.5 via Claude Code".
    pub name: String,
    /// What the heading over the limits of the account it runs under says, such as "Claude usage".
    pub usage_heading: String,
    /// The login a hub can hold for an account of this kind and hand to the workers it launches agents on. `None` for a
    /// kind a hub can hold none for, whose agents run under the login on their worker's machine, and from a hub older
    /// than this. It describes the kind's login, and doesn't say this hub keeps logins: one that doesn't refuses
    /// `account_connect` as `Unsupported`, which a client still has to expect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login: Option<LoginDescription>,
}

/// What the login a hub can hold for a kind of agent is: a token the agent's own CLI makes to last, which the person
/// makes themselves and gives the hub.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LoginDescription {
    /// The command the person runs to make the token, as they'd type it, such as `claude setup-token`.
    pub made_with: String,
    /// How long a token the command makes works for, if the CLI says, which is what [`HeldLogin::ends_at_ms`] is worked out
    /// from, since the CLI doesn't say when a given token ends.
    ///
    /// [`HeldLogin::ends_at_ms`]: super::super::HeldLogin::ends_at_ms
    #[serde(default)]
    pub lasts_ms: Option<i64>,
}

/// Why a chat can't be given a change to its settings.
#[derive(thiserror::Error, Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotOffered {
    #[error("the agent isn't offered that effort")]
    Effort,
    #[error("the agent isn't offered that approval mode, so a chat can't have it")]
    Mode,
    #[error("the agent does not offer those settings")]
    Settings,
}

impl AgentDescription {
    /// Whether it says what it offers only where a chat would launch, which the hub asks it, as
    /// [`Models::Discoverable`] says, rather than offering the same everywhere.
    pub fn discovers_models(&self) -> bool {
        matches!(self.capabilities.settings.models, Some(Models::Discoverable))
    }

    /// The models a chat is offered, in the order they're listed. None for a kind that discovers its own, until
    /// [`AgentDescription::with_catalog`] gives it what it offers where a chat would launch. A chat can run another,
    /// such as one named by its full ID, since what a name means is the agent's CLI's to say.
    pub fn models(&self) -> &[ModelOffer] {
        self.capabilities.settings.models.as_ref().map_or(&[], Models::offered)
    }

    /// The description where `catalog` applies, with its models in place of the kind's, and its default model as the
    /// one a chat there starts on.
    #[must_use]
    pub fn with_catalog(mut self, catalog: ModelCatalog) -> Self {
        self.capabilities.settings.models = Some(Models::Static(catalog));
        self
    }

    /// What a chat runs with until another is chosen, which the hub gives one that names none of them: the model its
    /// claims start a chat on, at that model's default effort, in the mode they give first. Only the mode, for a kind
    /// that discovers its models, until [`AgentDescription::with_catalog`] says them.
    pub fn defaults(&self) -> Settings {
        let catalog = self.capabilities.settings.models.as_ref().and_then(Models::catalog);
        let model = catalog.and_then(ModelCatalog::initial).map(|offer| offer.model.clone());
        let effort = model.as_ref().map(|model| self.effort_for_choice(model, None));
        Settings { model, mode: self.default_mode(), effort, ..Settings::default() }
    }

    /// The levels a model offered takes, least first, or for one that isn't offered, every level an offered one takes,
    /// since what an unlisted model takes is the agent's CLI's to say.
    pub fn efforts_for(&self, model: &ModelName) -> Vec<Effort> {
        self.effort_offers(model).into_iter().map(|e| e.id().clone()).collect()
    }

    /// The levels [`AgentDescription::efforts_for`] says, each with what the agent calls it.
    pub fn effort_offers(&self, model: &ModelName) -> Vec<EffortOffer> {
        if let Some(offer) = self.offer(model) {
            return offer.efforts.clone();
        }
        let mut every: Vec<EffortOffer> = Vec::new();
        for level in self.models().iter().flat_map(|offer| &offer.efforts) {
            if level.id().known().is_some() && !every.iter().any(|seen| seen.id() == level.id()) {
                every.push(EffortOffer::Id(level.id().clone()));
            }
        }
        every
    }

    pub fn offers_effort(&self, model: &ModelName, effort: &Effort) -> bool {
        let levels = self.efforts_for(model);
        if levels.is_empty() { effort == &Effort::Unset } else { effort.as_str().is_some() && levels.contains(effort) }
    }

    /// Keep a previously chosen effort if available, otherwise use the selected model's default, or for a model that
    /// isn't offered, the default of the one a chat starts on.
    pub fn effort_for_choice(&self, model: &ModelName, previous: Option<&Effort>) -> Effort {
        let levels = self.efforts_for(model);
        let starts_on = || {
            let catalog = self.capabilities.settings.models.as_ref().and_then(Models::catalog)?;
            catalog.initial()
        };
        let offer = self.offer(model).or_else(starts_on);
        let default = offer.and_then(|offer| offer.default_effort.as_ref());
        previous
            .filter(|e| levels.contains(e))
            .or_else(|| default.filter(|e| levels.contains(e)))
            .or_else(|| levels.first())
            .cloned()
            .unwrap_or(Effort::Unset)
    }

    /// Whether it compacts its conversation when asked, as `session_compact` asks, as well as by itself as its context
    /// fills.
    pub fn compacts(&self) -> bool {
        self.capabilities.context.compaction.is_some()
    }

    /// What the interface calls `model`, if it's one offered.
    pub fn label(&self, model: &ModelName) -> Option<&str> {
        self.offer(model).map(|offer| offer.label.as_str())
    }

    fn offer(&self, model: &ModelName) -> Option<&ModelOffer> {
        self.models().iter().find(|offer| offer.model == *model)
    }

    /// The approval modes a chat can be given, as its claims say, the one it starts in first.
    pub fn modes(&self) -> Vec<ApprovalMode> {
        let modes = self.capabilities.approvals.modes.as_ref();
        modes.map_or_else(Vec::new, |modes| [modes.default].into_iter().chain(modes.others.iter().copied()).collect())
    }

    /// The approval mode a chat starts in until another is chosen, as its claims say, if they offer any.
    pub fn default_mode(&self) -> Option<ApprovalMode> {
        self.modes().first().copied()
    }

    /// The approval mode a chat plans in, as its claims say, if it offers one.
    pub fn plan_mode(&self) -> Option<ApprovalMode> {
        self.modes().into_iter().find(|mode| mode.plan)
    }

    /// The approval mode for plan mode on, if `planning`, or off: the one a chat plans in, or the one it starts in.
    pub fn mode_for(&self, planning: bool) -> Option<ApprovalMode> {
        if planning { self.plan_mode() } else { self.default_mode() }
    }

    /// Whether a chat can be given what `settings` set: only a mode its claims offer and an effort its model is
    /// offered, never one this build has no name for, since neither could be passed on to the agent, and nothing else
    /// its claims don't take, as [`Settings::allowed`] says. Any model is taken, since the agent's CLI takes full IDs as
    /// well as the names offered, and says itself which it doesn't know.
    pub fn takes(&self, settings: &Settings) -> Result<(), NotOffered> {
        if settings.mode.is_some_and(|mode| !mode.is_known() || !self.modes().contains(&mode)) {
            return Err(NotOffered::Mode);
        }
        if let Some(effort) = &settings.effort
            && !self.discovers_models()
        {
            let allowed = match &settings.model {
                Some(model) => self.offers_effort(model, effort),
                None => {
                    effort.as_str().is_some()
                        && self.models().iter().any(|offer| offer.efforts.iter().any(|e| e.id() == effort))
                }
            };
            if !allowed {
                return Err(NotOffered::Effort);
            }
        }
        if !settings.allowed(&self.capabilities) {
            return Err(NotOffered::Settings);
        }
        Ok(())
    }
}
