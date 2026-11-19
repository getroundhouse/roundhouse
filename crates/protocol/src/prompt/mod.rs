//! What a person sends an agent, and what it has to be to go, sent or queued, so the hub and every client refuse the
//! same ones and say the same about them, and who wrote it, since the hub writes some for them.

mod from;
mod says;

pub use from::PromptFrom;
pub use says::{Prompt, PromptProblem, says_or_carries};
