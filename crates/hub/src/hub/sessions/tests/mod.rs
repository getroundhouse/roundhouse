//! The session requests' tests: what's let go of, kept and waited for of a chat, against a hub that never starts a
//! worker, and what a chat does with what it's asked, and what a change of a chat's settings does, on a hub whose
//! workspaces have stand-ins for their workers.

mod asking;
mod chats;
mod settings;
