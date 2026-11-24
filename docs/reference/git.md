# Git

Facts about git that took an experiment to find, or that more than one place in the code depends on. `worker::git` is the one module that runs git, as `docs/principles/outside-systems.md` says.

## Worktrees

- **Two git commands at once in one repository can fail while one of them is `git worktree add`.** `git worktree add` writes the new worktree's entry in `.git/worktrees/` one file at a time. A command that reads every worktree's entry fails if it finds an entry whose `commondir` file exists and isn't written yet, with `fatal: failed to read .git/worktrees/NAME/commondir`. `git worktree add` is such a command, because it looks for the branch in every other worktree, and `git fetch` is another. Seen on git 2.54. So a worker holds a lock around each of these commands, which `crates/worker/src/git/worktrees/` has.

## Diffs

A worker reads these in `crates/worker/src/git/changes/listing.rs`, for a change summary and for each page of the files past those it lists, and in `crates/worker/src/git/diff_file/`, for its diff of one file. Seen on git 2.39.

- **`git diff --raw --numstat -z` prints every file's raw entry, then every file's counts, in the same order.** A rename's raw entry ends with its status and score, such as `R100`, then the old path and the new one. Its counts have nothing after the second tab, then the two paths, each ended by a NUL.
- **git counts no lines of a file larger than `core.bigFileThreshold`, 512 MiB unless set,** and prints `-` for each count, as it does for a binary file.
- **A diff against a commit gives a file in conflict as `M`,** and not `U`.
- **A path after `--` is a pattern unless git is told `--literal-pathspecs`.** So `git diff HEAD -- 'a*.txt'` diffs `ab.txt` too.
- **A rename is found only when both its paths are named.** `git diff --find-renames <base> -- new.txt` gives `new.txt` as added, and naming `old.txt` before it gives the rename.
- **A file taken out of the index and left in the worktree, as `git rm --cached` leaves it, is two changes:** `git diff <base>` gives it as deleted, and `git ls-files --others` lists it as untracked, so a summary lists its path twice.
- **`git diff --no-index` exits with 1 when the two files differ,** and reads `/dev/null` as no file. Its `--numstat` prints `-` for each count of a binary file, as a diff against a commit does.

## What git ignores

A worker's watch of a worktree reads these, in `crates/worker/src/git/ignored.rs`. Seen on git 2.39.

- **`git ls-files --others --ignored --exclude-standard --directory -z` lists the topmost folders git ignores whole, each ending in `/`, and the files it ignores in folders it doesn't,** such as `target/`, `node_modules/` and `pkg/build/` for a `build` pattern. An empty folder it ignores is listed too. A folder holding a file git tracks isn't listed whole, and its ignored files are, one by one.
- **`git check-ignore -z --stdin` says a path inside an ignored folder is ignored,** such as `target/debug/deep`, so a new folder's own folders can be asked about without listing again. It exits 1, not 0, when none of what it was asked is ignored.
- **git's own reads raise no write that a watch hears,** with `GIT_OPTIONAL_LOCKS=0`, which the worker runs every git with, since `diff` and `status` then don't write the index back. On Linux they still open and read files, which inotify tells of as access, and which a watch has to drop.

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

## A repository's variables

- **git before 2.40 lists `GIT_INTERNAL_SUPER_PREFIX` among a repository's variables, in `git rev-parse --local-env-vars`,** and git 2.40 and later don't read it. git set it in a submodule's command to the submodule's path. With it set, git 2.39 refuses every command but the few that recurse into submodules, such as `git status`, with "status doesn't support --super-prefix". So the worker's list of a repository's variables, in `crates/worker/src/git/repository_variables.rs`, keeps it.
