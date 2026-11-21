//! The session requests' tests: what's let go of, kept and waited for of a chat, against a hub that never starts a
//! worker, and what a chat does with what it's asked, what a change of a chat's settings does, and what its record says
//! of its agent's work in the background and of each subagent, and what a read of its events is sent, on a hub whose
//! workspaces have stand-ins for their workers, and what a launch tells the agent of its workspace.

mod asking;
mod background;
mod chats;
mod conversations;
mod cover_lines;
mod launching;
mod reading;
mod reading_out;
mod settings;
mod subagents;
