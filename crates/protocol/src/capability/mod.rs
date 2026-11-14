//! What an agent can do, as one typed set that every adapter translates its agent into.
//!
//! A capability is something an agent does, for the person or for Roundhouse, whose behavior is specified here once for
//! every agent: carrying a conversation over to a new process, taking a prompt mid-turn, asking the person a question,
//! saying how full its context is. The set is the experience Roundhouse decides is best, and each adapter fits its
//! agent to it where it can, rather than every agent being held to what they share, so what only one agent can do,
//! only its kind claims. Its adapter claims what its agent can do, and translates the agent into each claim. What the
//! agent can't do is left out of its claims: the hub refuses it, and a client doesn't offer it.
//!
//! - [`Capability`] names each one, by area. The doc of each variant of an area's own `Capability` is its contract:
//!   what the adapter guarantees once it claims it, whatever its agent does underneath.
//! - [`Capabilities`] is what a run claims. Each launch reports its own, with its kind's as the default, since what an
//!   agent can do depends on its CLI's version and flags, the account and the model provider. A claim carries a
//!   parameter only where the hub or a client acts on it, such as whether a rewind puts files back. Any other way two
//!   agents differ is the adapter's to hide.
//! - [`calls::Call`] is what a tool call does, typed by its kind, and [`calls::CallOutput`] is what it sent back, so a
//!   client reads neither from an agent's own format.
//! - [`conversation::Start`] is how a launch starts its conversation, with [`settings::Settings`], [`Control`] is
//!   everything the hub asks of a running agent, and [`Report`] is what an agent says that
//!   [`EventBody`](crate::event::EventBody) has no kind for. Each names the capabilities it needs, so the hub refuses
//!   a request a run doesn't claim before it's sent, and drops a report its run doesn't claim. A launch is held to its
//!   kind's claims, since its run hasn't reported its own.
//! - `laws`, which only tests build, checks a session's log against the contracts a log can show by itself, such as
//!   each prompt being settled once and nothing being recorded that its run doesn't claim.
//!
//! Translating can take more than renaming. Codex never asks to have a plan approved: it writes the plan and ends its
//! turn. Its adapter holds that turn's end back, asks for the plan's approval itself, and on approval starts Codex's
//! next turn out of plan mode, as the same turn, so the person gets the same card from every agent. Codex has no tool
//! for reading a file either, and reads with commands, which it parses, so its adapter records what Codex read each
//! command to do with the command, and a client draws a `cat` as reading a file. opencode stops a turn when the person
//! refuses a tool without saying why, so its adapter gives a refusal its own words, and the agent carries on, as a
//! plain refusal does for every agent.
//!
//! Some things agents offer aren't capabilities, because Roundhouse owns their state: the queue of prompts, how
//! conversations are named, grouped, archived and marked read, sharing one on the web, worktrees, a subagent's
//! included, signing in to an account, which the person does with the agent's own CLI and hands the hub, and reading
//! or running anything in the worktree for a client. An agent's copy of any of them would be a second record of what
//! the hub already records. Talking with an agent by voice is left out too, since no client has a way to. So are an
//! agent's memories, and the schedules it keeps past its process, which carry from one conversation to the next on the
//! worker's machine, where Roundhouse has no record of them and an instance that comes and goes loses them, so an
//! adapter turns them off where its agent lets it.
//!
//! What these docs say of Claude is of its CLI at 2.1.323. What they say of Codex at 0.167.0 and opencode at 1.18.30
//! is read from their source and their servers' schemas, rather than seen running.
//!
//! "What each kind of agent can do" in `docs/architecture.md` has the plan that puts this in place of
//! [`AgentFeature`], the efforts and modes in an [`AgentDescription`], [`CallKind`](crate::event::CallKind), the
//! worker's requests of an agent, and what the methods of the `agents` crate's `AgentProcess` say of themselves.
//!
//! [`AgentFeature`]: crate::model::AgentFeature
//! [`AgentDescription`]: crate::model::AgentDescription

pub mod account;
pub mod approvals;
mod area;
pub mod asking;
pub mod background;
pub mod calls;
mod capabilities;
pub mod context;
mod control;
pub mod conversation;
#[cfg(feature = "testing")]
pub mod laws;
pub mod output;
pub mod prompts;
mod report;
pub mod settings;
pub mod setup;
pub mod turns;

pub use capabilities::{Capabilities, Capability, Claimed};
pub use control::Control;
pub use report::{Keeper, Progress, Report};

#[cfg(test)]
mod tests;
