//! A workspace's warnings: what's wrong that the workspace can't put right itself, said in the card its name opens,
//! with what to do about it, such as GitHub showing the default no repository where the workspace's branch's pull
//! requests are, which the pull request's bar shows too, since it's why the bar can't say where the pull request
//! stands.

#[cfg(test)]
mod tests;

mod drawn;
mod words;

pub(crate) use drawn::{OpenGit, card, drawn, sign};
pub(crate) use words::{Answer, Warning, sentence};
