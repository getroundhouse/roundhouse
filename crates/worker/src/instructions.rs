//! What every agent is told about working in Roundhouse, and each of its subagents, in their system prompts, as
//! `docs/architecture.md` says under "What agents are told". It's written for any agent, and each adapter passes it to
//! its own. It tells an agent when to use the worker's tools rather than `gh`, as well as the tool server's own
//! instructions, in `crate::tools`, do, since those are a section about the server, which a subagent sees only after its
//! first tool call. It isn't Markdown, since the checks pass over a change that touches only Markdown, and this is built
//! into the worker.

use protocol::model::AgentKind;

/// What every agent is told.
const FOR_EVERY_AGENT: &str = include_str!("instructions.txt");

/// What every agent is told of the worker's tools, worded to hold whether or not it has them, since a chat keeps the
/// system prompt it was first given until it's compacted, and the tools are given at each launch. It holds for every
/// version of the tool server, whose tools differ by the version the agent's CLI speaks.
const OF_THE_TOOLS: &str = include_str!("instructions_tools.txt");

/// What an agent that hears how a tool call it left in the background ended is told of waiting with the tools. One that
/// doesn't, as Codex doesn't, is offered no wait by the tool server, so it isn't told to wait with them.
const OF_WAITING: &str = include_str!("instructions_waits.txt");

/// What an agent of `kind` is told, each part as a paragraph of its own.
pub(crate) fn instructions(kind: AgentKind) -> String {
    let mut told = format!("{FOR_EVERY_AGENT}\n{OF_THE_TOOLS}");
    if can_wait(kind) {
        told = format!("{told}\n{OF_WAITING}");
    }
    told
}

/// Whether `kind`'s adapter claims
/// [`ToolTasks`](protocol::capability::background::Capability::ToolTasks), which a wait with the tools needs.
fn can_wait(kind: AgentKind) -> bool {
    agents::descriptions()
        .into_iter()
        .find(|description| description.kind == kind)
        .is_some_and(|description| description.capabilities.background.tool_tasks.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_dir::Folder;

    #[test]
    fn the_instructions_name_each_folder_the_worker_makes() {
        for folder in Folder::ALL {
            let named = format!("`{}/`", folder.path());
            assert!(instructions(AgentKind::Codex).contains(&named), "{folder:?} isn't named as the worker makes it");
        }
    }

    #[test]
    fn every_agent_is_sent_to_the_tools_and_only_one_that_can_wait_with_them_is_told_to() {
        let (claude, codex) = (instructions(AgentKind::Claude), instructions(AgentKind::Codex));

        assert!(
            codex.starts_with(FOR_EVERY_AGENT) && codex.contains(".\n\nWhen Roundhouse serves you tools"),
            "{codex}"
        );
        assert!(!codex.contains("`gh run watch`") && !codex.contains("end your turn"), "never to wait: {codex}");
        assert!(claude.starts_with(&codex) && claude.contains(".\n\nWhen what you do next waits"), "{claude}");
        assert!(claude.ends_with("the agent that started it waits.\n"), "{claude}");
    }

    #[test]
    fn only_a_kind_that_hears_how_a_call_left_in_the_background_ended_can_wait() {
        assert!(can_wait(AgentKind::Claude), "Claude's CLI runs a wait as an MCP task");
        assert!(!can_wait(AgentKind::Codex), "Codex's speaks a version of MCP without tasks");
        assert!(!can_wait(AgentKind::Antigravity), "Antigravity's CLI does not negotiate MCP tasks");
    }
}
