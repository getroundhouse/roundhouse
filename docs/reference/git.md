# Git

Facts about git that took an experiment to find, or that more than one place in the code depends on. `worker::git` is the one module that runs git, as `docs/principles/outside-systems.md` says.

## Worktrees

- **Two git commands at once in one repository can fail while one of them is `git worktree add`.** `git worktree add` writes the new worktree's entry in `.git/worktrees/` one file at a time. A command that reads every worktree's entry fails if it finds an entry whose `commondir` file exists and isn't written yet, with `fatal: failed to read .git/worktrees/NAME/commondir`. `git worktree add` is such a command, because it looks for the branch in every other worktree, and `git fetch` is another. Seen on git 2.54. So a worker holds a lock around each of these commands, which `crates/worker/src/git/worktrees/` has.

## Config given in the environment

- **git 2.31 is the first to read config from `GIT_CONFIG_COUNT`, `GIT_CONFIG_KEY_<n>` and `GIT_CONFIG_VALUE_<n>`.** It reads them again on every run, as the command's own config, which wins over the repository's and the person's. So a file named there as `include.path` can be rewritten under a process that's running, and its next `git` reads the new one. A worker gives every process its git config this way (`Files::environment`, in `crates/worker/src/git_credential/files.rs`), and asks its git's version once (`reads_the_config`).
- **A `GIT_CONFIG_COUNT` of 1 with no `GIT_CONFIG_KEY_0` fails every `git`,** with "missing config key GIT_CONFIG_KEY_0". `docs/reference/codex-cli.md` has a Codex setting that leaves a command like that.

## Config for only some repositories

A worker's git config sets who commits are by only in a repository with a remote on github.com, with `includeIf "hasconfig:remote.*.url:<pattern>"`. `Files::config` and `identity_config`, in `crates/worker/src/git_credential/files.rs`, depend on each of these.

- **git 2.36 is the first to know `hasconfig:remote.*.url`.** An older git takes a condition it doesn't know as false, and includes nothing, as git 2.31 and 2.35 do.
- **git matches each remote's address as the repository's config has it, before any `insteadOf`.** So a pattern for a host has to name every form its addresses take, such as `https://github.com/`, `https://<user>@github.com/`, `git@github.com:` and `ssh://git@github.com/`.
- **Any remote that matches counts,** not only `origin`.
- **In a pattern, `*` matches no `/`, and `**` matches one only when it comes after a `/`.** So `git@github.com:**` matches none of GitHub's addresses, and `git@github.com:*/**` matches them all: an owner, then the rest.
- **A worktree's remotes are its repository's,** so the condition holds in a worktree as it does in the checkout.
- **A file included under the condition can't hold a remote's address.** Every git fails on one, as git 2.54 does.
- **git passes over an include of a file that isn't there.** So removing the included file is enough to stop including it.
