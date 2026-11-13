//! What a chat runs with, which the person chooses: its model, effort, speed, persona and budget, and how a change
//! reaches a running agent.

mod budgets;
mod chosen;
mod claims;
mod live;
mod models;
mod personas;

pub use budgets::{Budget, BudgetUnit, Budgets};
pub use chosen::{Settings, Verbosity};
pub use claims::{Capability, Claims, Models};
pub use live::{LiveChanges, Takes};
pub use models::{Effort, Input, ModelOffer, Price, Speed, Stage};
pub use personas::{Persona, PersonaFor, PersonaName, StyleName};
