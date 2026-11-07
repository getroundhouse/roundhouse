//! The workspace commands.

use anyhow::{anyhow, bail};
use client_core::Client;
use protocol::model::Workspace;

use crate::args::WorkspaceCmd;
use crate::run::short;

pub(crate) fn workspace_command(client: &Client, command: WorkspaceCmd) -> anyhow::Result<()> {
    match command {
        WorkspaceCmd::Create { repo, name, base } => {
            let repos = client.repo_list()?;
            let found = repos
                .iter()
                .find(|r| r.name == repo || r.id == repo)
                .or_else(|| repos.iter().find(|r| r.id.starts_with(&repo)))
                .ok_or_else(|| anyhow!("no repository matches {repo}. Add one with `trestle repo add <path>`"))?;
            let workspace = client.workspace_create(&found.id, name, base)?;
            println!("{}", workspace.name);
            println!("  path    {}", workspace.path.unwrap_or_default());
            println!("  branch  {}", workspace.branch);
            println!("  base    {}", workspace.base_ref.unwrap_or_default());
        }
        WorkspaceCmd::List { all } => {
            for workspace in client.workspace_list(all)? {
                println!(
                    "{}  {}  {:?}  {}",
                    short(&workspace.id),
                    workspace.name,
                    workspace.status,
                    workspace.path.unwrap_or_default()
                );
            }
        }
        WorkspaceCmd::Archive { workspace, force } => {
            let found = find_workspace(client, &workspace)?;
            client.workspace_archive(&found.id, force)?;
            println!("archived {}. Branch {} was kept", found.name, found.branch);
        }
        WorkspaceCmd::Restore { workspace } => {
            let found = find_workspace(client, &workspace)?;
            let restored = client.workspace_restore(&found.id)?;
            println!("restored {} at {}", restored.name, restored.path.unwrap_or_default());
        }
        WorkspaceCmd::Status { workspace } => {
            let found = find_workspace(client, &workspace)?;
            let status = client.workspace_status(&found.id)?;
            println!("on {}", status.branch.as_deref().unwrap_or("detached HEAD"));
            for file in status.files {
                println!("{}  {}", file.status, file.path);
            }
        }
        WorkspaceCmd::Diff { workspace } => {
            let found = find_workspace(client, &workspace)?;
            let diff = client.workspace_diff(&found.id)?;
            print!("{}", diff.patch);
            if diff.truncated {
                eprintln!("(some untracked files were left out)");
            }
        }
    }
    Ok(())
}

pub(crate) fn find_workspace(client: &Client, query: &str) -> anyhow::Result<Workspace> {
    let workspaces = client.workspace_list(true)?;
    let mut matches: Vec<_> = workspaces.iter().filter(|w| w.name == query || w.id == query).collect();
    if matches.is_empty() {
        matches = workspaces.iter().filter(|w| w.id.starts_with(query)).collect();
    }
    match matches.as_slice() {
        [one] => Ok((*one).clone()),
        [] => bail!("no workspace matches {query}"),
        many => bail!("{query} matches {} workspaces. Use more of the ID", many.len()),
    }
}
