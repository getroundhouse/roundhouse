//! A request to use a tool, asked in the composer's place as a question of the panel's own, which the agent never
//! asked: to allow the use or deny it, with a row to type what the agent should do instead, which denies it in those
//! words. Between the question and its answers it shows what the tool was given, such as a command, whole, or what an
//! edit would change, and why the agent asks, when it says. What the request says of the use, and what each answer
//! means, are `client_core::ToolUseAsked`'s and `client_core::ToolUseAnswer`'s.

#[cfg(test)]
mod tests;

mod choices;
pub(super) use choices::INSTEAD;
#[cfg(test)]
use choices::Shown;
pub(crate) use choices::ToolUse;
