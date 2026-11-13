# Coming from Conductor

A repository set up for Conductor works in Roundhouse unchanged. Roundhouse reads Conductor's settings and sets its variables under Conductor's own names, and has no names of its own for them.

## What works unchanged

- **Settings files:** `.conductor/settings.toml`, `.conductor/settings.local.toml` and `~/.conductor/settings.toml`.
- **Scripts:** `setup` and `archive`, under `[scripts]`.
- **Files to copy:** `.worktreeinclude`, then `file_include_globs`, then `.env*` when neither is set.
- **Variables:** `CONDUCTOR_WORKSPACE_NAME`, `CONDUCTOR_WORKSPACE_ID`, `CONDUCTOR_WORKSPACE_PATH`, `CONDUCTOR_ROOT_PATH`, `CONDUCTOR_DEFAULT_BRANCH`, `CONDUCTOR_PORT` and `CONDUCTOR_IS_LOCAL`, for scripts and for agents.

[Repository settings](../reference/repository-settings.md) has the details of each.

## What isn't read

- Run scripts, and `run_mode`.
- Managed settings.

## What's the same idea

The model is the one you know: a workspace for each task, with a worktree and a branch of its own, agents side by side, and a diff and a pull request beside each chat. [What Roundhouse does](what-it-does.md) lists what's here today, and the [roadmap](../roadmap.md) says what isn't built.

Roundhouse is a product of its own, and reading these settings is for compatibility, nothing more. It doesn't read Conductor's workspaces or chats, so start with the [quickstart](quickstart.md).
