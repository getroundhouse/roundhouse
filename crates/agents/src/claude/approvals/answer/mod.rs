//! What the CLI is told of what the person decided about a request, in its own form: the tool's input for an approval,
//! the answers spliced into it for questions, and the words for a refusal, which are the adapter's to choose when the
//! person gave none.

#[cfg(test)]
mod tests;

mod response;
#[cfg(test)]
use response::{DENIED, KEEP_PLANNING, NOT_ANSWERED};
pub(in crate::claude) use response::{Waiting, denied_in_its_own_words, response};
