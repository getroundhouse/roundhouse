//! What a chat runs with, which the person chooses: its model, effort, speed, persona and budget, and how a change
//! reaches a running agent.

mod budgets;
mod chosen;
mod claims;
mod effort_id;
mod live;
mod models;
mod personas;

pub use crate::model::Effort;
pub use budgets::{Budget, BudgetUnit, Budgets};
pub use chosen::{Settings, Verbosity};
pub use claims::{Capability, Claims, Models};
pub use effort_id::EffortId;
pub use live::{LiveChanges, Takes};
pub use models::{EffortOffer, Input, ModelCatalog, ModelOffer, Price, Speed, Stage};
pub use personas::{Persona, PersonaFor, PersonaName, StyleName};
