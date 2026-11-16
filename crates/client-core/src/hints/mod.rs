//! Gathering the words a dictated prompt or answer is likely to say from what the client already holds of the
//! workspace, repository, open session and question.

mod collect;
mod english;
mod prose;

pub use collect::HintCollector;

#[cfg(test)]
mod tests;
