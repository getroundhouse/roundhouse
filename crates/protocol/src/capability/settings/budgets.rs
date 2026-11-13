//! A chat's budget, and what it can be counted in.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Budgets {
    /// What a budget can be counted in. One in any other unit is refused.
    pub units: Vec<BudgetUnit>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BudgetUnit {
    Tokens,
    /// Millionths of a US dollar.
    MicroUsd,
    /// The agent's own requests to its model.
    Requests,
    #[serde(other)]
    Unknown,
}

/// A chat's budget, as [`Capability::Budgets`](super::Capability::Budgets) says.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    pub most: u64,
    pub unit: BudgetUnit,
}
