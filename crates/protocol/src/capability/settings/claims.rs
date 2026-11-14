//! What a run claims of a chat's settings.

use serde::{Deserialize, Serialize};

use super::{Budgets, LiveChanges, ModelOffer};
use crate::capability::Claimed;
use crate::capability::area::area;

area! { "a chat's settings";
    /// The models [`Models::offered`] lists can be chosen, each saying what it takes, and a model named by its full ID
    /// can be too, which the agent says whether it knows. Every chat names a model, so this is only what's offered.
    Models => models: Models,
    /// The models are found by asking the agent, since they depend on its account, its model providers and its
    /// project's configuration, as opencode's do, rather than built into the adapter. The hub asks the chat's agent,
    /// and keeps the answer on the session's record, in place of the kind's list for that chat, so a model added to
    /// the account or the project shows without a new build.
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
pub struct Models {
    /// In the order they're listed.
    pub offered: Vec<ModelOffer>,
}
