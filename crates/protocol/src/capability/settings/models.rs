//! The models a chat is offered, and what each takes.

use serde::{Deserialize, Serialize};

use super::Effort;
use crate::model::ModelName;

/// All models offered in one launch context. Reading the catalog never starts a turn.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ModelCatalog {
    pub offered: Vec<ModelOffer>,
    /// The one a chat starts on, or `None` for the first offered.
    pub default_model: Option<ModelName>,
}

impl ModelCatalog {
    /// The model a chat starts on: the default, if it's offered, or else the first offered.
    pub fn initial(&self) -> Option<&ModelOffer> {
        let default = self.default_model.as_ref().and_then(|id| self.offered.iter().find(|offer| offer.model == *id));
        default.or_else(|| self.offered.first())
    }
}

/// The adapter's level, with its display name when different from its usual label.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum EffortOffer {
    Named { id: Effort, label: String },
    Id(Effort),
}

impl EffortOffer {
    pub fn id(&self) -> &Effort {
        match self {
            Self::Named { id, .. } | Self::Id(id) => id,
        }
    }

    pub fn label(&self) -> Option<&str> {
        match self {
            Self::Named { label, .. } => Some(label),
            Self::Id(id) => id.label(),
        }
    }
}

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
    pub efforts: Vec<EffortOffer>,
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
