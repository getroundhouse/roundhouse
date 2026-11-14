use super::super::{AgentKind, Effort, ModelName, PermissionMode, SessionSettings, SessionSettingsChange};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RunCapabilities {
    pub run: super::super::RunId,
    pub claims: Box<crate::capability::Capabilities>,
}

/// A kind of agent, as its adapter describes it: what the interface calls it, the models, efforts and approval modes a
/// chat can be given, what a chat runs with until another is chosen, what it does that not every agent does, and what
/// login a hub can hold for it. The hub serves one for each kind it runs, so a client offers what the agent takes rather
/// than holding a list of its own.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentDescription {
    pub kind: AgentKind,
    /// The adapter's launch defaults. A running agent reports its own in `AgentInit`; absence means a peer from
    /// before capability claims, whose legacy controls remain available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<crate::capability::Capabilities>,
    /// What answered a turn, as its card says, such as "Opus 5.5 via Claude Code".
    pub name: String,
    /// What the heading over the limits of the account it runs under says, such as "Claude usage".
    pub usage_heading: String,
    /// The models a chat is offered, in the order they're listed. A chat can run another, such as one named by its
    /// full ID, since what a name means is the agent's CLI's to say.
    pub models: Vec<ModelChoice>,
    /// The levels a chat is offered, least first. One from a newer hub reads as [`Effort::Unknown`], which a client
    /// can't name and doesn't offer.
    pub efforts: Vec<Effort>,
    /// The approval modes a chat can be given.
    pub modes: Vec<PermissionMode>,
    /// What a chat runs with until another is chosen, which the hub gives one that names none of them. Each is one of
    /// those offered.
    pub defaults: SessionSettings,
    /// What it does that not every agent does. Whatever a hub leaves out, the agent doesn't do.
    #[serde(default)]
    pub features: Vec<AgentFeature>,
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

/// A model a chat is offered, by the name the agent's CLI takes, such as an alias, and what the interface calls it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ModelChoice {
    pub model: ModelName,
    pub label: String,
}

/// Something an agent does that not every agent does.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentFeature {
    /// It compacts its conversation when asked, as `session_compact` asks, as well as by itself as its context fills.
    Compacts,
    /// One from a newer hub, which this build can't act on.
    #[serde(other)]
    Unknown,
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
    /// The selected model's levels when its catalog says, otherwise the kind's legacy levels.
    pub fn efforts_for(&self, model: &ModelName) -> Vec<Effort> {
        let offered = self
            .capabilities
            .as_ref()
            .and_then(|claims| claims.settings.models.as_ref())
            .and_then(|models| models.offered.iter().find(|offer| offer.model == *model));
        self.efforts
            .iter()
            .copied()
            .filter(|effort| {
                effort.known().is_some() && offered.is_none_or(|offer| offer.efforts.contains(&(*effort).into()))
            })
            .collect()
    }

    /// Keep an effort when the new model takes it, or use that model's default.
    pub fn effort_when_switching(&self, model: &ModelName, current: Effort) -> Effort {
        let efforts = self.efforts_for(model);
        if efforts.contains(&current) {
            return current;
        }
        let default = self
            .capabilities
            .as_ref()
            .and_then(|claims| claims.settings.models.as_ref())
            .and_then(|models| models.offered.iter().find(|offer| offer.model == *model))
            .and_then(|offer| offer.default_effort);
        efforts
            .iter()
            .copied()
            .find(|effort| Some((*effort).into()) == default)
            .or_else(|| efforts.first().copied())
            .unwrap_or(self.defaults.effort)
    }

    pub fn has(&self, feature: AgentFeature) -> bool {
        if let Some(claims) = &self.capabilities {
            return match feature {
                AgentFeature::Compacts => claims.context.compaction.is_some(),
                AgentFeature::Unknown => false,
            };
        }
        self.features.contains(&feature)
    }

    /// What the interface calls `model`, if it's one offered.
    pub fn label(&self, model: &ModelName) -> Option<&str> {
        self.models.iter().find(|choice| choice.model == *model).map(|choice| choice.label.as_str())
    }

    /// Whether a chat can be given what `change` sets: only an effort and a mode it's offered, and never one this build
    /// has no name for, since neither could be passed on to the agent. Any model is taken, since the agent's CLI takes
    /// full IDs as well as the names offered, and says itself which it doesn't know.
    pub fn takes(&self, change: &SessionSettingsChange) -> Result<(), NotOffered> {
        if let Some(claims) = &self.capabilities {
            let settings = change.for_capabilities(claims)?;
            if !settings.allowed(claims) {
                return Err(NotOffered::Settings);
            }
        }
        if let (Some(model), Some(effort)) = (&change.model, change.effort)
            && !self.efforts_for(model).contains(&effort)
        {
            return Err(NotOffered::Effort);
        }
        let offered = |effort: Effort| effort.known().is_some_and(|effort| self.efforts.contains(&effort));
        if change.effort.is_some_and(|effort| !offered(effort)) {
            return Err(NotOffered::Effort);
        }
        let offered = |mode: PermissionMode| mode != PermissionMode::Unknown && self.modes.contains(&mode);
        if change.permission_mode.is_some_and(|mode| !offered(mode)) {
            return Err(NotOffered::Mode);
        }
        Ok(())
    }
}
