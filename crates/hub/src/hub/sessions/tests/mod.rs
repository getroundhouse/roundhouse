//! The session requests' tests: what's let go of, kept and waited for of a chat, against a hub that never starts a
//! worker, and what a chat does with what it's asked, what a change of a chat's settings does, and what its record says
//! of its agent's work in the background and of each subagent, on a hub whose workspaces have stand-ins for their
//! workers.

mod asking;
mod background;
mod chats;
mod settings;
mod subagents;
