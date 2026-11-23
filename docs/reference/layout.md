# Layout

A map for finding your way before reading code: the parts and what connects them, what each crate is, who owns each piece of state, and which kind each thing a hub holds is. `docs/principles/parts.md` has the rules the parts follow, and `docs/principles/layers.md` which crate can depend on which.

## The parts

```
 CLIENTS
 ┌─────────────┐  ┌─────┐  ┌────────────┐
 │ Desktop app │  │ CLI │  │ iPhone app │
 └──────┬──────┘  └──┬──┘  └─────┬──────┘
        └────────────┼───────────┘
  client-core: each hub's records, open sessions and open terminals, kept in sync
                     │
                     │  client protocol: JSON lines over a Unix socket, a named pipe on
                     │  Windows, or TLS to a paired hub (a relay for phones, not built)
                     ▼
 HUBS: one client can connect to several
 ┌─────────────────────────────────────┐  ┌────────────────────────────┐
 │ Hub on your machine                 │  │ Hub on your own server     │
 │ owns workspaces, sessions and their │  │ same code and API, and its │
 │ event logs, in SQLite               │  │ own adapters (not built)   │
 └────────┬────────────────────────────┘  └────────────────────────────┘
          │  provider protocol: JSON lines over stdio
          ▼
 PROVIDER ADAPTERS, children of the hub, which make and end instances
 ┌──────────────────┐ ┌───────────────┐ ┌───────────────┐ ┌──────────┐
 │ local (inside    │ │ Incus         │ │ EC2           │ │ your own │
 │ the hub for now) │ │ (not built)   │ │ (not built)   │ │          │
 └────────┬─────────┘ └───────┬───────┘ └───────┬───────┘ └──────────┘
          ▼                   ▼                 ▼
 INSTANCES, one per workspace, each running one worker
 ┌──────────────────┐ ┌───────────────┐ ┌───────────────┐
 │ a directory and  │ │ a VM, cloned  │ │ an EC2        │
 │ processes on     │ │ from a base   │ │ instance from │
 │ this machine     │ │ image on ZFS  │ │ an AMI        │
 └────────┬─────────┘ └───────┬───────┘ └───────┬───────┘
          └───────────────────┼─────────────────┘
                              ▼
 WORKERS  ◀── worker protocol, from the hub: JSON lines over the stdio of the command
              `connect` returns. The hub runs it, so no adapter sits in a worker's stream
 ┌──────────────────────────────────────────────────────────────────────┐
 │ `roundhoused worker`, one per workspace                              │
 │ git and the worktree, setup and archive scripts, files and diffs     │
 │ terminals, and agent adapters, such as `agents::claude`              │
 │ its agents' tools, over MCP on 127.0.0.1, answered by asking the hub │
 └──────────────────────────────────┬───────────────────────────────────┘
                                    │  stream-json or JSON-RPC over stdio
                                    ▼
 AGENTS
 ┌────────────────────────────────────────────┐  ┌──────────────────────────┐
 │ Claude, Codex or Antigravity CLI: your own │  │ other agents (not built) │
 │ install, settings, on the worker's machine │  │                          │
 └─────────────────────┬──────────────────────┘  └──────────────────────────┘
                       ▼
 MODEL PROVIDERS: Anthropic, OpenAI, Google, or a cloud's API
```

## The crates

One Cargo workspace. Each crate's own doc comment says more.

```
crates/
  protocol/        The client, worker and provider protocols' types
  hub/             The hub
  github/          The hub's calls to GitHub
  worker/          The worker
  agents/          An adapter for each agent's CLI
  providers/       The trait a provider implements, and the local provider
  process/         Starting, signalling and recording processes
  private-files/   Files and sockets only their user can open
  client-core/     A client's connection, and its copy of each hub's state
  local-hub/       Starting a hub on this machine, for the desktop app and the CLI
  transcript/      A chat's history and live snapshot, in GPUI
  client-ui/       What every app draws of workspaces and chats but the transcript, such as their marks, in GPUI
  terminal-state/  The worker's copy of each terminal, on Ghostty's terminal core
  terminal-view/   A terminal, in GPUI
  look/            The design system: colours, type, sizes, components and icons
  dictation/       Speech to text on the device
  dictation-ui/    Dictation in a text field, in GPUI
  now-playing/     Pausing what's playing while dictating, on a Mac from inside perl
  fetching/        Downloading files over HTTP
  update-feed/     What a release adds to the update feed
  windows-icon/    An icon as the resource a Windows program carries, for the desktop app's build
  video/           Decoding the WebM videos agents make, with libvpx
  libvpx-build/    What video's build script compiles of libvpx, for a target
  waiting/         How tests wait
  spec-screens/    The spec's screens, for the galleries that draw them: approving a picture, and what the screens' chats say
apps/
  daemon/          `roundhoused`, which runs as a hub or a worker
  cli/             `roundhouse`, the command-line client
  desktop/         `roundhouse-desktop`, the desktop app
  mobile-ios/      `roundhouse-ios`, the iPhone app
tools/             Checks, builds and packaging, as `docs/reference/commands.md` lists
third_party/       Copies of other projects' code, as `third_party/README.md` says
```

## Who owns what

`docs/principles/state.md` has the rule and its test. This is where each piece of state ends up.

| Owner | What it owns |
|---|---|
| The hub | Sessions and their logs, workspaces, comments, proposals, waits, drafts, what's read, its settings, and which instances it asked for |
| A workspace's worker, and its machine | Running agents and terminals, and the worktree's files |
| A provider | Whether an instance exists, and whether it's running |
| A client | What's open and scrolled where, the app's settings, the window's place, and dictation |
| Git and GitHub | Code, pull requests and CI |

## Which kind each thing a hub holds is

Everything a hub holds or serves, which of the four kinds in `docs/principles/serving.md` it is, where it's kept and how a client gets it. Something new takes its kind by the same test: the hub owns it, so it's a record, or the hub only observes it, so it's an observation. What an agent says in an event is the log's, so what the hub works out from it is a record, and what an agent says outside the log is an observation. For example, a session's status is worked out from its events, so it's a record, and how full its agent's context is comes in a report that isn't an event, so it's an observation.

A table named here is in the hub's database, except `facets`, `observed_workspace_lines`, `observed_workspace_changes`, `observed_referenced`, `observed_workspace_pull_request`, `observed_workspace_pull_request_detail` and `observed_workspace_repository_unseen`, which are in the cache file `github-cache.db`. Each is sent at the level its row names, to a client that follows that level through `Subscribe`: where a level that holds records or observations stands first, as a list, and then each change. A client of version 3 follows each through a request of its own for its level: `FeedSubscribe` and `RecordsList` for the hub's, `SessionSubscribe` and the lists by session for a session's, `TerminalWatch` for a terminal's, and `GithubSubscribe` for what GitHub says of a workspace's own pull request and of a chip. A start with `github_observed` is sent what GitHub says of a workspace's own pull request and of a chip as observations, in place of the old form, as `crate::server::sends` decides, and any other start none of them.

### Events and live snapshots

| What | Kind | Kept in | Sent by |
|---|---|---|---|
| A session's events, and the files and images they carry | Event | `events` and `attachments`, and the attachment files | A session's level, and the requests that read a log, such as `SessionRead` |
| A session's tail: what its agent and subagents are writing, its compacting, its retry of a failed request, what's said for it in conversation mode, and how far a device has read a reply out loud | Live snapshot | The hub's memory | A session's level, whole and then each change |
| What a terminal shows | Live snapshot | The worker, with the newest picture in the hub's memory | A terminal's level |

### Records

| What | Why it's the hub's | Kept in | Sent by |
|---|---|---|---|
| Repositories: name, checkout and when added | Roundhouse added each | `repos` | The hub's level |
| Which repository a project is, kept as the address a worker read once from its checkout's `origin` | Roundhouse's link | `repos.url` | The hub's level, in the repository's `origin` |
| Workspaces: name, base, worktree's path, status, provider, instance, setup, ports, title, when made and archived | Roundhouse made each, and decides each | `workspaces` | The hub's level |
| The branch a workspace started on, and the branch an archived one had checked out | Roundhouse chose the first, and recorded the second as the worktree went | `workspaces.branch` | The hub's level |
| Which pull request is a workspace's, and the branch it was found on | Roundhouse's decision, from the workspace's branches and what GitHub says | `workspaces.pull_request_link` and `pull_request_branch` | The hub's level |
| A workspace's other worktrees, which its agents make: path, the branch it was made on, the session and agent that made it, status, when made | Roundhouse made each, as an agent asked | `child_worktrees` | No client. The agents' tools return them |
| A session's agent, model, mode, effort, title and queue, its account and how it was put on it | Chosen by a person, or by the hub. For an agent on its machine's own login, the account is whose the worker says that login is | `sessions` | The hub's level |
| A session's status, its newest reply and how it ended, when its turn started, its open approvals, background tasks, prompts not taken, whether its queue is held, and whether its agent opened | Worked out from its log, in the same transaction as the event | `sessions` | The hub's level |
| What the run that's going claims it can do | Worked out from its log too: said in the event that starts the run, and taken off with the one that ends it | `sessions.agent_capabilities` | The hub's level |
| A session's subagents | Worked out from its log, in the same transaction as the event | `subagents` | A session's level, listed and then each change |
| A session's waits on pull requests: what each waits for, what it saw as it started, its deadline and how it ended | Asked for by an agent, through its tools | `waits` | The hub's level, on the session's record |
| Proposals, each revision, and the answers to them | What agents and people entered through Roundhouse | `proposals` and `proposal_revisions` | A session's level, listed and then each change, and `ProposalRevisionsRead` |
| Drafts: a session's composer and its answer to an approval, a comment on a workspace's diff, and the new workspace with its files | What a person typed | `sessions.draft`, `sessions.decision_draft`, `workspaces.comment_draft`, `new_workspace_draft`, `new_workspace_draft_files` and the files | The hub's level |
| Comments on a workspace's diff | What a person entered | `diff_comments` | The hub's level |
| How far each chat is read, and when it and its workspace were last worked in | One person's, kept for them | `sessions.read_seq`, and the `user_active_at_ms` of `sessions` and `workspaces` | The hub's level |
| In conversation mode, which device has each chat's conversation, and how each reply read out ended | One person's, kept for them | `sessions.conversation` and `replies_heard` | The hub's level, and a session's for the replies |
| Hub settings, each agent kind's default account, and whether Automatic puts its chats on accounts | Chosen by a person | `hub_settings` and `account_defaults` | The hub's level, on the settings' record |
| The default GitHub account, what the hub sends workers for it, and the providers it sets GitHub up on or leaves it to | Chosen by a person, or worked out by the hub from its logins and its providers file | `hub_settings` | The hub's level, on the settings' record |
| Agents' accounts: the name a person gave one, its held login, its reserve and when it was last used | Roundhouse's, and what it holds | `accounts`, and the login files | The hub's level |
| GitHub accounts: the GitHub user each is, by its ID, its held login, and the app installation it is | Roundhouse's, and what it holds | `github_accounts`, and the login files | The hub's level |
| Apps of your own, by their ID, with their keys | Roundhouse made each, through GitHub | `github_apps`, and the key files | The hub's level |
| Sign-ins with GitHub | Roundhouse's, so every client shows the same code | `github_sign_ins` | The hub's level |
| Paired devices: name, when paired and when last seen | Roundhouse's, and what it holds | `devices` | `DeviceList` |
| A workspace's open terminals | A person opened each. Kept while its shell runs | The hub's memory | The hub's level |
| How an update a person asked for is going, on a hub installed as a service | The hub's own work, which lasts only as long as the hub | The hub's memory, on its `HubUpdate` record | The hub's level |

A hub of this build lists no `FoundGithubLogin`, which `client-core` reads only from an older hub.

### Observations

*Not built:* any observation but a workspace's lines added and removed, its changed files and what GitHub says of a chip's issue or pull request and of a workspace's own, so each of the others is kept as its row says: as a record or a field of one, in GitHub's cache, which reaches a client with no revision, or not at all.

| What | Whose | Kept in | Sent by |
|---|---|---|---|
| A workspace's lines added and removed | Git's | The hub's memory and `observed_workspace_lines`, each with when it was read. The hub reads them from the worker as a turn ends in the workspace and as its branches move, and while a client follows the workspace its worker, if it's running, watches the worktree and sends them as they change. Forgotten as the workspace is archived | The hub's level, as an observation, listed and then each change |
| A workspace's changed files, each with its lines and version | Git's | The hub's memory and `observed_workspace_changes`, each with when it was read, from the same read as the lines, and current with them while watched. Forgotten as the workspace is archived. Not kept: the files past a summary's 5,000, and the diff. The worker reads those, and one file's diff, in pages, and the whole diff at once | The workspace's level, as an observation, listed and then each change. `WorkspaceChangedFiles`, `WorkspaceDiff` and `WorkspaceFileDiff` |
| How many files a workspace has uncommitted | Git's | `workspaces.pull_request_link`, as of the hub's last read of the workspace's pull request | The hub's level |
| A workspace's branches, and each of its other worktrees' | Git's | `workspaces.branches` and `child_worktrees.branches` | The hub's level, for a workspace's own. Another worktree's reaches no client |
| Which build a workspace's worker is | The worker's | `workspaces.worker_build` | The hub's level |
| What GitHub says of an issue or a pull request: its summary, checks, stack, when a review last asked for changes and when it last left the merge queue, and its branch's rules for checks | GitHub's | The hub's memory and `facets`, each facet with when it was read, under one version | An issue's or pull request's level, and the hub's for a workspace's own, with no revision, and `ReferencesRead` and `WorkspacePullRequest`. The rules only say which checks are required |
| What GitHub says of an issue or a pull request, as a chip draws it | Worked out from GitHub's | The hub's memory and `observed_referenced`, worked out from `facets` while a client follows it, and gone once `facets` forgets it | An issue's or pull request's level, as an observation, to a start with `github_observed`, and otherwise as the row above says |
| How a workspace's pull request stands: the pull request, its summary, and how far its worktree is ahead and behind | Worked out from git's and GitHub's | The hub's memory and `observed_workspace_pull_request`, worked out from the workspace's record and `facets` as either changes | The hub's level, as an observation, to a start with `github_observed`, and otherwise as `own_pull_request` |
| A workspace's pull request's checks, history and stack, of the head they were read at | GitHub's | The hub's memory and `observed_workspace_pull_request_detail`, worked out as the row above | The workspace's level, as an observation, to a start with `github_observed`, and otherwise in `own_pull_request` at the hub's |
| That GitHub showed the default GitHub account no repository where a workspace's pull request would be | GitHub's | The hub's memory, and `observed_workspace_repository_unseen` for a workspace it ever showed none | The workspace's level, as an observation, to a start with `github_observed`, and otherwise in `own_pull_request` at the hub's |
| The pull requests each wait watches, as last read | GitHub's | `waits.watch` | The hub's level, on the session's record |
| Whether the default GitHub account can push to a repository | GitHub's | `github_reach`, a record | The hub's level |
| A GitHub account's username, name and email | GitHub's | `github_accounts.username`, and `github_profiles` for the name and email | The hub's level, for the username. The name and email reach no client, and go to workers as git's author for commits |
| What GitHub calls an app of your own, its owner and its bot, and the user or organization an installation is on | GitHub's | `github_apps`, and `github_accounts.installed_on` | The hub's level |
| An agent's list of models | The agent's CLI | The hub's memory, as a record | The hub's level, and `ModelsFor` |
| Who an agent's account is: its email, plan, model provider and how it's logged in | The provider's, read from the agent's login by a worker at each launch | `accounts` | The hub's level |
| An account's usage limits | The provider's, said by an agent | `account_limits` | The hub's level, on the account's record |
| How full an agent's context is, and its todos | The agent's | `sessions.context` and `sessions.todos` | The hub's level, on the session's record |
| What the API's cache holds of a conversation | The provider's, said by an agent of each of its requests | `sessions.prompt_cache` | The hub's level, on the session's record |
| The release a hub installed as a service would update to | The update feed's | The hub's memory, on its `HubUpdate` record | The hub's level |
| GitHub's limit on each token | GitHub's | The hub's memory | No client |

### Read on request, and not kept

The hub reads these from a worker, or from its own machine, each time a client asks, and keeps nothing of them: one file's diff and the whole diff, the pages of the changed files past those a summary lists, the file list, a file's lines, a folder, the setup log, git's status for an archive, a terminal's rows and text, who `gh` and the SSH key on the hub's machine are, and whose login a session's agent runs under (`SessionAccount`). It answers from its memory with its status (`HubStatus`), what each kind of agent it runs offers (`AgentList`), and a code to pair a device, which it keeps only until the code expires.

### The hub's own, served to no client

| What | Kept in |
|---|---|
| The counters that revisions and run IDs come from | `counters` |
| The hub's ID, which it gives its providers | `hub_identity` |
| The instances it asked a provider for | `instances` |
| Each workspace's tools port, and each session's token for its tools | `tools_ports` and `tools_tokens` |
| Which login each run is under | `runs_under_logins` |
| Each request that made something, so one sent twice is answered the same | `client_request_id` of `workspaces`, `sessions` and `diff_comments`, `events.client_msg_id`, and the ask keys of proposals and other worktrees |
| What a workspace is made from, and where its worker runs, as settled when it was made | `workspaces.repo_source` and `worker_environment` |
| Whether a repository's address is to be read from its checkout | `repos.url_to_read` |
| The agent's own ID for a session's conversation, and how far the hub recorded each run's events | `sessions.agent_session_id`, `recorded_run` and `recorded_through` |
| When a session's draft was typed, so the newest typing wins | `sessions.draft_typed_at_ms` |
| Where each attachment was put in the worktree, and whether a client added it | `attachments.placed_at` and `by_client` |
| When each usage limit was heard | `account_limits.heard_at_ms` |
| Where each login, key and device token is | `login_file` of `accounts` and `github_accounts`, `github_apps.key_file` and `made_from`, and `devices.token_sha256` |
| What each client shows of GitHub, and when the hub reads GitHub next | The hub's memory |
