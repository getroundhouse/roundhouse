//! What every agent is told about working in Roundhouse, and each of its subagents, in their system prompts. It's
//! written for any agent, and each adapter passes it to its own. It tells an agent when to use the worker's tools
//! rather than `gh`, as well as the tool server's own instructions, in `crate::tools`, do, since those are a section
//! about the server, which a subagent sees only after its first tool call. It isn't Markdown, since the checks pass over
//! a change that touches only Markdown, and this is built into the worker.
//!
//! `docs/principles/agents.md` has the rule this text is worded by, under "The worker's tools". Rules in the spec say
//! what an agent is told, such as to show an image from `.context/`, in `docs/spec/chats.md`, to wait on a pull request
//! with the tools, in `docs/spec/workspaces.md`, when to propose, in `docs/spec/proposals.md`, and how to link to an
//! issue or a pull request, in `docs/spec/chats.md`, so a change to this text may change a rule there.

use protocol::model::{AgentKind, GithubRepositoryName};

/// What every agent is told, before how to link to an issue or a pull request, which names its workspace's repository.
const FOR_EVERY_AGENT: &str = include_str!("instructions.txt");

/// What a link to an issue or a pull request does, after an example of one.
const LINKS_DRAWN: &str = "rather than writing its number alone, and the app draws it as a chip that says what it is \
                           and where it stands, in place of the link's words.";

/// What every agent is told of the worker's tools, worded to hold whether or not it has them, since a chat keeps the
/// system prompt it was first given until it's compacted, and the tools are given at each launch. It holds for every
/// version of the tool server, whose tools differ by the version of MCP the agent's CLI speaks. So it doesn't say which
/// pull requests the tools cover, and leaves that to the server's own instructions.
const OF_THE_TOOLS: &str = include_str!("instructions_tools.txt");

/// What an agent that hears how a tool call it left in the background ended is told of waiting with the tools. One that
/// doesn't, as Codex doesn't, is offered no wait by the tool server, so it isn't told to wait with them.
const OF_WAITING: &str = include_str!("instructions_waits.txt");

/// What an agent of `kind` is told, in a workspace whose repository on GitHub is `repository`, each part as a paragraph
/// of its own.
pub(crate) fn instructions(kind: AgentKind, repository: Option<&GithubRepositoryName>) -> String {
    let mut told = format!("{FOR_EVERY_AGENT}\n{}\n\n{OF_THE_TOOLS}", links(repository));
    if can_wait(kind) {
        told = format!("{told}\n{OF_WAITING}");
    }
    told
}

/// How to link to an issue or a pull request, by its repository's real owner and name, which an agent left to work
/// them out guesses, as from whoever commented on an issue. Named for one in the workspace's repository, since that's
/// what most links are to, and otherwise taken from the repository's address.
fn links(repository: Option<&GithubRepositoryName>) -> String {
    match repository {
        Some(repository) => format!(
            "This workspace's repository is `{repository}` on GitHub. To mention an issue or a pull request on GitHub, \
             link to it there, such as `[#1021](https://github.com/{repository}/pull/1021)` for one of this \
             repository's, {LINKS_DRAWN} Link to one in another repository by that repository's own owner and name."
        ),
        None => format!(
            "To mention an issue or a pull request on GitHub, link to it there, such as \
             `[#1021](https://github.com/owner/name/pull/1021)`, {LINKS_DRAWN} Take the owner and name from the \
             address of the repository it's in, such as `git remote -v` shows for this one, rather than from anyone's \
             login."
        ),
    }
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
            let told = instructions(AgentKind::Codex, None);
            assert!(told.contains(&named), "{folder:?} isn't named as the worker makes it");
        }
    }

    #[test]
    fn every_agent_is_sent_to_the_tools_and_only_one_that_can_wait_with_them_is_told_to() {
        let (claude, codex) = (instructions(AgentKind::Claude, None), instructions(AgentKind::Codex, None));

        assert!(
            codex.starts_with(FOR_EVERY_AGENT) && codex.contains(".\n\nWhen Roundhouse serves you tools"),
            "{codex}"
        );
        assert!(!codex.contains("`gh run watch`") && !codex.contains("end your turn"), "never to wait: {codex}");
        assert!(claude.starts_with(&codex) && claude.contains(".\n\nWhen what you do next waits"), "{claude}");
        assert!(claude.ends_with("the agent that started it waits.\n"), "{claude}");
    }

    #[test]
    fn an_agent_is_told_its_workspaces_repository_on_github_to_link_to() {
        let told = instructions(AgentKind::Claude, Some(&GithubRepositoryName::named("cli/cli")));

        assert!(told.contains("This workspace's repository is `cli/cli` on GitHub."), "{told}");
        assert!(told.contains("`[#1021](https://github.com/cli/cli/pull/1021)`"), "{told}");
        assert!(!told.contains("owner/name"), "nothing's left to guess: {told}");
        assert!(told.contains("link's words. Link to one in another repository"), "{told}");
    }

    #[test]
    fn an_agent_whose_repository_isnt_on_github_is_told_where_an_owner_and_name_come_from() {
        let told = instructions(AgentKind::Claude, None);

        assert!(!told.contains("This workspace's repository is"), "{told}");
        assert!(told.contains("`[#1021](https://github.com/owner/name/pull/1021)`"), "{told}");
        assert!(
            told.contains("such as `git remote -v` shows for this one, rather than from anyone's login."),
            "{told}"
        );
    }

    #[test]
    fn only_a_kind_that_hears_how_a_call_left_in_the_background_ended_can_wait() {
        assert!(can_wait(AgentKind::Claude), "Claude's CLI runs a wait as an MCP task");
        assert!(!can_wait(AgentKind::Codex), "Codex's speaks a version of MCP without tasks");
        assert!(!can_wait(AgentKind::Antigravity), "Antigravity's CLI does not negotiate MCP tasks");
    }
}
