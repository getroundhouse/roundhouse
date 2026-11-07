# Inspiration

What we've learned from Conductor, the app that inspired this one, from its docs. It's a source of ideas, not a spec, and no code or assets are copied. New users can no longer sign up or download it, so the installed copy is the only way left to try it.

## Product model

From Conductor's docs (`/docs/concepts/workflow`):

- One shippable unit maps to one workspace, one branch, and typically one PR.
- A workspace has its own branch, working tree, files, app processes and agent context.
- The lifecycle is: break down the work, create workspaces, run agents independently, verify and review and resolve, then open a PR, merge and archive.
- Durable guidance lives in repository settings or instruction files. Task context comes from chat, attachments and `.context`.

## Compatibility

What a repository set up for Conductor can use, so it can work here too. This is a compatibility feature, and the project has no names of its own for these yet. Run actions and some of the settings aren't read so far:

- `.conductor/settings.toml`, `.conductor/settings.local.toml`, `~/.conductor/settings.toml`. Precedence: managed, then repo local, repo shared, user, defaults.
- `[scripts]` with `setup`, `archive`, `run_mode`, and `[scripts.run.<id>]` with `command`, `icon`, `default`, `available_in`.
- `.worktreeinclude`, then `file_include_globs`, then the default `.env*` pattern, for files to copy.
- Env vars `CONDUCTOR_WORKSPACE_NAME`, `_ID`, `_PATH`, `CONDUCTOR_ROOT_PATH`, `CONDUCTOR_DEFAULT_BRANCH`, `CONDUCTOR_PORT` (ten ports per workspace), `CONDUCTOR_IS_LOCAL`.
- The `.context` directory for sharing between agents.

## Sources

Conductor's docs run to 68 pages, listed at `conductor.build/llms.txt`. A private reference copy and all 209 changelog entries are in `.context/conductor-docs/`, which is the best source for new `small-features` issues. It's gitignored, and only exists in the `yaounde` workspace. Don't republish it.

- Conductor: [announcement](https://www.conductor.build/announcement), [changelog](https://conductor.build/changelog), [workflow](https://conductor.build/docs/concepts/workflow), [API](https://www.conductor.build/docs/api), [cloud FAQ](https://www.conductor.build/docs/cloud/faq), [docs index](https://conductor.build/llms.txt)
