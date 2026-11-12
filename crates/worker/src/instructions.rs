//! What every agent is told about working in Roundhouse, and each of its subagents, in their system prompts, as
//! `docs/architecture.md` says under "What agents are told". It's written for any agent, and each adapter passes it to
//! its own. It says only what holds whether or not the agent has the worker's tools: what those are for is in the
//! tool server's own instructions, in `crate::tools`. It isn't Markdown, since the checks pass over a change that
//! touches only Markdown, and this is built into the worker.

pub(crate) const INSTRUCTIONS: &str = include_str!("instructions.txt");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_dir::Folder;

    #[test]
    fn the_instructions_name_each_folder_the_worker_makes() {
        for folder in Folder::ALL {
            let named = format!("`{}/`", folder.path());
            assert!(INSTRUCTIONS.contains(&named), "{folder:?} isn't named as the worker makes it, {named}");
        }
    }
}
