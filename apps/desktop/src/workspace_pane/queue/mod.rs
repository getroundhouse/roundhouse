//! The queue above the composer: prompts waiting for the agent to finish, which can be changed or taken out until the hub
//! sends them. `rows` draws it.

mod rows;

mod editing;
pub(super) use editing::Editing;
