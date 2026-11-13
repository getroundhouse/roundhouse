//! The models a chat is offered, and what each takes.

use serde::{Deserialize, Serialize};

use crate::model::ModelName;

/// A model a chat is offered, and what it takes.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ModelOffer {
    /// The name the agent takes, such as an alias.
    pub model: ModelName,
    /// What the interface calls it.
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Who serves it, for an agent that runs several model providers' models, as opencode's `providerID` says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_provider: Option<String>,
    /// Least first. Empty for a model with no choice of effort.
    #[serde(default)]
    pub efforts: Vec<Effort>,
    /// What it thinks at when no effort is chosen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_effort: Option<Effort>,
    /// Empty for a model with one speed.
    #[serde(default)]
    pub speeds: Vec<Speed>,
    /// What a prompt can carry to it besides text.
    #[serde(default)]
    pub inputs: Vec<Input>,
    /// The most its context holds, when the agent says before a conversation does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price: Option<Price>,
    #[serde(default)]
    pub stage: Stage,
}

/// How hard a model thinks before it answers. The union of every agent's levels, each model offering its own, least
/// first, in [`ModelOffer::efforts`]. It has no order of its own, since one from a newer peer can't be placed.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    /// It doesn't reason before it answers.
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
    /// Codex's level above the most, which also hands work to subagents by itself.
    Ultra,
    /// Codex's `persistent`, which some of its models offer.
    Persistent,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Speed {
    Standard,
    /// Faster, and costs more of the account's limits, as Claude's fast mode and Codex's priority tier do.
    Fast,
    /// Slower, and costs less, as Codex's flex tier does.
    Flex,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Input {
    Images,
    Audio,
    Documents,
    Video,
    #[serde(other)]
    Unknown,
}

/// What a model costs, in millionths of a US dollar for each million tokens, as its model provider lists it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Price {
    pub input_micro_usd: u64,
    pub output_micro_usd: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_micro_usd: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write_micro_usd: Option<u64>,
}

/// How settled a model is, as its model provider says.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    #[default]
    Current,
    /// Early, and may change or go.
    Preview,
    /// On its way out.
    Deprecated,
    #[serde(other)]
    Unknown,
}
