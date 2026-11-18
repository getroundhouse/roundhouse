//! What a run claims of a chat's settings.

use serde::{Deserialize, Serialize};

use super::{Budgets, LiveChanges, ModelCatalog, ModelOffer};
use crate::capability::Claimed;
use crate::capability::area::area;
use crate::model::ModelName;

area! { "a chat's settings";
    /// The models [`Models::offered`] lists can be chosen, each saying what it takes, and a model named by its full ID
    /// can be too, which the agent says whether it knows. Every chat names a model, so this is only what's offered,
    /// and which a chat starts on.
    Models => models: Models,
    /// A running agent can send a replacement catalog with [`Report::Models`](crate::capability::Report::Models)
    /// and answer [`Control::ListModels`](crate::capability::Control::ListModels). Pre-session discovery is
    /// advertised independently by [`Models::Discoverable`].
    DiscoveredModels => discovered_models: Claimed,
    /// How hard the model thinks can be chosen, from the efforts each model offers.
    Effort => effort: Claimed,
    /// How fast the model answers can be chosen, from the speeds each model offers, such as a fast one that uses more
    /// of the account's limits. The agent reports the speed it runs at whenever that isn't the one chosen, as when
    /// Claude's fast mode cools down, and again once it is, which the hub keeps on the session's record.
    Speed => speed: Claimed,
    /// A persona, the agent's name for a set of instructions, tools and rules, such as opencode's `build` or a
    /// project's own, can be chosen for a chat, from those the agent lists whole whenever they change, which the hub
    /// keeps on the session's record.
    Personas => personas: Claimed,
    /// A style for the agent's replies, such as one that explains as it goes, can be chosen, from those the agent lists
    /// whole whenever they change, which the hub keeps on the session's record.
    Styles => styles: Claimed,
    /// How much the agent's replies say can be chosen, from little to a lot, as Codex's `model_verbosity` does.
    Verbosity => verbosity: Claimed,
    /// A chat can be given a budget, in one of the units [`Budgets::units`] lists, and the agent stops once it's spent,
    /// ending the turn as over budget.
    Budgets => budgets: Budgets,
    /// A stronger model can be chosen for the agent to ask for advice as it works, from the models the kind offers,
    /// as Claude's `--advisor` does, and each time it asks is a call of its own, [`Call::Advisor`].
    ///
    /// [`Call::Advisor`]: crate::capability::calls::Call::Advisor
    Advisor => advisor: Claimed,
    /// A change of a setting [`LiveChanges`] has reaches a running agent, which agrees before the hub saves it, and
    /// takes it when that setting's [`Takes`](super::Takes) says. A change of any other waits for the next launch.
    LiveChanges => live_changes: LiveChanges,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(from = "CatalogSource", into = "CatalogSource")]
pub enum Models {
    /// The agent says what it offers where a chat would launch, before any chat there starts, which the hub asks it
    /// and keeps as a [`ModelList`](crate::model::ModelList).
    Discoverable,
    /// The same everywhere, in the order they're listed, with the one a chat starts on.
    Static(ModelCatalog),
}

// Older peers read the offered field of both forms, including an empty list before discovery.
#[derive(Serialize, Deserialize)]
struct CatalogSource {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    discoverable: bool,
    offered: Vec<ModelOffer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    default_model: Option<ModelName>,
}

impl From<CatalogSource> for Models {
    fn from(source: CatalogSource) -> Self {
        let CatalogSource { discoverable, offered, default_model } = source;
        if discoverable { Self::Discoverable } else { Self::Static(ModelCatalog { offered, default_model }) }
    }
}

impl From<Models> for CatalogSource {
    fn from(models: Models) -> Self {
        match models {
            Models::Static(ModelCatalog { offered, default_model }) => {
                Self { discoverable: false, offered, default_model }
            }
            Models::Discoverable => Self { discoverable: true, offered: Vec::new(), default_model: None },
        }
    }
}

impl Models {
    pub fn offered(&self) -> &[ModelOffer] {
        self.catalog().map_or(&[], |catalog| &catalog.offered)
    }

    /// What's offered, and what a chat starts on, unless the agent says only where a chat would launch.
    pub fn catalog(&self) -> Option<&ModelCatalog> {
        match self {
            Self::Static(catalog) => Some(catalog),
            Self::Discoverable => None,
        }
    }
}
