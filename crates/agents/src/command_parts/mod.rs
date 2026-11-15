//! What a command does, read from its own words, for a client to draw one that only prints a file, lists files or
//! searches what's in them as doing that. A command does that only where it's one program that does nothing else,
//! given only what's known to do nothing more, so it's [`CommandPart::Other`] wherever it does more or this can't
//! tell, and its parts never say it does less than it does.
//!
//! An agent's own reading of its commands isn't used. Codex's, the only one an agent sends, sums a command up for
//! Codex's own interface and leaves out what it does beyond that, as "What a command does" in `docs/codex-cli.md`
//! says. What no reading of a command's words can see is what its shell or its program is set up to do, such as a
//! function in a shell's startup file, or a flag in `rg`'s config file.

mod command;
mod flags;
mod programs;
#[cfg(test)]
mod tests;
mod words;

pub(crate) use command::of;
