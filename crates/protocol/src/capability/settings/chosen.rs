//! What a chat runs with, and whether given claims take it.

use serde::{Deserialize, Serialize};

use super::{Budget, BudgetUnit, Capability, Effort, LiveChanges, PersonaName, Speed, StyleName, Takes};
use crate::capability::Capabilities;
use crate::capability::approvals::ApprovalMode;
use crate::capability::output::ThinkingForm;
use crate::model::ModelName;

/// What a chat runs with. A launch names the model, and every other setting the kind claims, so none is left to the
/// agent's own configuration, and leaves out only what the kind doesn't claim. A change names only what it changes.
/// It takes the place of `SessionSettings` and `SessionSettingsChange`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct Settings {
    pub model: Option<ModelName>,
    pub mode: Option<ApprovalMode>,
    pub effort: Option<Effort>,
    pub speed: Option<Speed>,
    pub thinking: Option<ThinkingForm>,
    pub persona: Option<PersonaName>,
    pub style: Option<StyleName>,
    pub verbosity: Option<Verbosity>,
    pub advisor: Option<ModelName>,
    pub budget: Option<Budget>,
}

impl Settings {
    /// The capabilities a launch with them needs. A model needs none, since every chat names one.
    pub fn needs(&self) -> Vec<crate::capability::Capability> {
        let Settings { model: _, mode, effort, speed, thinking, persona, style, verbosity, advisor, budget } = self;
        [
            mode.map(|_| crate::capability::approvals::Capability::Modes.into()),
            effort.map(|_| Capability::Effort.into()),
            speed.map(|_| Capability::Speed.into()),
            thinking.map(|_| crate::capability::output::Capability::Thinking.into()),
            persona.as_ref().map(|_| Capability::Personas.into()),
            style.as_ref().map(|_| Capability::Styles.into()),
            verbosity.map(|_| Capability::Verbosity.into()),
            advisor.as_ref().map(|_| Capability::Advisor.into()),
            budget.map(|_| Capability::Budgets.into()),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    /// Whether an agent of a kind that claims `claims` can be launched with them: every capability they need, a mode,
    /// a form of thinking and a budget's unit the kind offers, and no effort, speed or verbosity this build doesn't
    /// know, which the adapter couldn't tell its agent. Which efforts and speeds a model offers is the model's, which
    /// the hub checks against the chat's model.
    pub fn allowed(&self, claims: &Capabilities) -> bool {
        let Settings { model: _, mode, effort, speed, thinking, persona: _, style: _, verbosity, advisor: _, budget } =
            self;
        let offers_mode = |mode| claims.approvals.modes.as_ref().is_some_and(|modes| modes.offers(mode));
        let offers_thinking = |form| claims.output.thinking.as_ref().is_some_and(|thinking| thinking.offers(form));
        let counts = |budget: Budget| {
            budget.unit != BudgetUnit::Unknown
                && claims.settings.budgets.as_ref().is_some_and(|budgets| budgets.units.contains(&budget.unit))
        };
        claims.has_all(self.needs())
            && mode.is_none_or(offers_mode)
            && thinking.is_none_or(offers_thinking)
            && budget.is_none_or(counts)
            && *effort != Some(Effort::Unknown)
            && *speed != Some(Speed::Unknown)
            && *verbosity != Some(Verbosity::Unknown)
    }

    /// Whether a running agent whose run claims `claims` can be changed to them: as [`Settings::allowed`] says, and
    /// only in settings it takes a change of.
    pub fn allowed_live(&self, claims: &Capabilities) -> bool {
        let Some(live) = &claims.settings.live_changes else { return false };
        let LiveChanges {
            model: takes_model,
            mode: takes_mode,
            effort: takes_effort,
            speed: takes_speed,
            thinking: takes_thinking,
            persona: takes_persona,
            style: takes_style,
            verbosity: takes_verbosity,
            advisor: takes_advisor,
            budget: takes_budget,
        } = *live;
        let Settings { model, mode, effort, speed, thinking, persona, style, verbosity, advisor, budget } = self;
        let takes = |set: bool, takes: Option<Takes>| !set || takes.is_some();
        self.allowed(claims)
            && takes(model.is_some(), takes_model)
            && takes(mode.is_some(), takes_mode)
            && takes(effort.is_some(), takes_effort)
            && takes(speed.is_some(), takes_speed)
            && takes(thinking.is_some(), takes_thinking)
            && takes(persona.is_some(), takes_persona)
            && takes(style.is_some(), takes_style)
            && takes(verbosity.is_some(), takes_verbosity)
            && takes(advisor.is_some(), takes_advisor)
            && takes(budget.is_some(), takes_budget)
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Verbosity {
    Low,
    Medium,
    High,
    #[serde(other)]
    Unknown,
}
