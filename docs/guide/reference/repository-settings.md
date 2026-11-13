# Repository settings

What Roundhouse reads from a repository: scripts, the files to copy into a new worktree, and the variables it sets. The file names and variables are Conductor's, so a repository set up for it works unchanged.

## The settings files

Three files are read, and the first to set a value wins:

| File | For |
|---|---|
| `.conductor/settings.local.toml`, in the repository | Your own overrides, not committed |
| `.conductor/settings.toml`, in the worktree | What the repository commits, on the workspace's branch |
| `~/.conductor/settings.toml` | Your defaults for every repository |

A file that doesn't parse is skipped with a warning, so a typo doesn't stop a workspace.

## What a file can set

```toml
file_include_globs = """
.env*
config/local.yml
"""

[scripts]
setup = "npm install"
archive = "docker compose down"
```

| Setting | What it does |
|---|---|
| `scripts.setup` | Runs once a workspace's worktree exists, and again when it's restored |
| `scripts.archive` | Runs as a workspace is archived |
| `file_include_globs` | Which ignored files to copy into a new worktree, a pattern to a line |

Scripts run with `bash`, in the worktree, with the variables below.

Run scripts, `run_mode` and managed settings aren't read.

## Files to copy

A new worktree has only what git tracks. Roundhouse copies ignored files into it from the repository, by the first of these that's there:

1. `.worktreeinclude`, at the repository's root.
2. `file_include_globs`, in the settings.
3. `.env*`.

Patterns mean what they do in `.gitignore`, one to a line. A line that starts with `#` is a comment.

## Variables

Set for both scripts, and for every agent in the workspace.

| Variable | What it holds |
|---|---|
| `CONDUCTOR_WORKSPACE_NAME` | The workspace's name, such as `holly` |
| `CONDUCTOR_WORKSPACE_ID` | The workspace's ID |
| `CONDUCTOR_WORKSPACE_PATH` | The worktree's path |
| `CONDUCTOR_ROOT_PATH` | The repository's path |
| `CONDUCTOR_DEFAULT_BRANCH` | The branch the workspace was made from, such as `main` |
| `CONDUCTOR_PORT` | The first of the workspace's ten ports |
| `CONDUCTOR_IS_LOCAL` | `1` |

## Folders Roundhouse uses in a worktree

| Folder | What's in it |
|---|---|
| `.context/attachments/` | The files prompts carried |
| `.context/tmp/` | Where agents are told to keep what they make for their own use, such as screenshots |

Git is told to ignore them, so nothing in them shows as a change.
