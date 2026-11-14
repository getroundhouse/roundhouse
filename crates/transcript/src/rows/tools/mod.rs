//! A tool call, and the result it got back, drawn together. They arrive as separate events, often with other calls
//! in between when the agent runs several at once, so each call's result is found in the log by [`super::around`].
//! What each kind of call looks like is in the modules below, all on the same [`super::line`].

use super::around::{Answer, Outcome, Reply, Subagent};

mod advisor;
pub(super) mod agent;
pub(super) mod block;
mod command;
mod files;
mod highlight;
mod marks;
mod numbered;
mod other;
mod search;
mod typed;
mod web;

use super::line;
pub(crate) use files::ChangeAsked;

mod calls;
pub(super) use calls::{Toggle, amount, call, result, shown, shown_result};
