//! The rule each write the hub repeats keeps: writing what a record holds already leaves its revision, and the hub's,
//! where they were. A client takes only a copy with a newer revision, so a revision that moved for nothing would send
//! every client a record it has, each time the hub hears the same again, as it does on every read of GitHub, git or an
//! agent's output.

mod github;
mod hub;
mod rule;
mod sessions;
mod workspaces;
