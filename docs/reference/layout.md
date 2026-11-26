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
 │ the hub's tool servers beside them, whose calls it passes on         │
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
  tool-sign-in/    The hub's signing in to MCP servers, as an OAuth client, asking one for its tools, and reading their refusals, which the worker does too
  worker/          The worker
  agents/          An adapter for each agent's CLI
  providers/       The trait a provider implements, and the local provider
  process/         Starting, signalling and recording processes
  private-files/   Files and sockets only their user can open
  client-core/     A client's connection, its copy of each hub's state, and what an app keeps on its device
  local-hub/       Starting a hub on this machine, for the desktop app and the CLI
  logging/         How every process logs: JSON lines, levels, panics, and keeping a log short
  hint-words/      The words a dictation is biased toward, and what they were picked from
  kept-recordings/ The files a dictation someone chose to keep is written as
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
  written-counts/  A count as a person reads it, whole or shortened, for every client and for look
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
| A client | What's open and scrolled where, the app's settings and preferences, the window's place, dictation, and, of a sign-in to a tool server, what it's to, how it stands and what listens for where the browser is sent back, until it ends, and the reloads of a chat's MCP servers it has asked for, until the agent says more of them |
| Git and GitHub | Code, pull requests and CI |

## Which kind each thing a hub holds is

Everything a hub holds or serves, which of the four kinds in `docs/principles/serving.md` it is, where it's kept and how a client gets it. Something new takes its kind by the same test: the hub owns it, so it's a record, or the hub only observes it, so it's an observation. What an agent says in an event is the log's, so what the hub works out from it is a record, and what an agent says outside the log is an observation. For example, a session's status is worked out from its events, so it's a record, and how full its agent's context is comes in a report that isn't an event, so it's an observation.

A table named here is in the hub's database, except `facets`, `observed_workspace_lines`, `observed_workspace_changes`, `observed_referenced`, `observed_workspace_pull_request`, `observed_workspace_pull_request_detail`, `observed_workspace_repository_unseen`, `observed_repository_reach`, `observed_github_username`, `observed_github_app_said`, `observed_github_app_bot`, `observed_installed_on`, `observed_model_catalog`, `observed_account_identity`, `observed_account_limits`, `observed_session_context`, `observed_session_todos`, `observed_session_tool_servers`, `observed_session_cache`, `observed_wait_read` and `observed_github_profile`, which are in the cache file `github-cache.db`. Each is sent at the level its row names, to a client that follows that level through `Subscribe`: where a level that holds records or observations stands first, as a list, and then each change. The hub speaks versions 5 and 6 of the client protocol, and clients speak 6. Version 6 reads everything as 5 does, so a start on a connection of either is sent every observation at its level, and the same.

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
| Which pull request is a workspace's, and the branch it was found on | Roundhouse's decision, from the workspace's branches and what GitHub says. Not decided again while its branches aren't known | `workspaces.pull_request_link` and `pull_request_branch` | The hub's level |
| A workspace's other worktrees, which its agents make: path, the branch it was made on, the session and agent that made it, status, when made | Roundhouse made each, as an agent asked | `child_worktrees` | No client. The agents' tools return them |
| A session's agent, model, mode, effort, title and queue, its account and how it was put on it | Chosen by a person, or by the hub. For an agent on its machine's own login, the account is whose the worker says that login is | `sessions` | The hub's level |
| A session's status, its newest reply and how it ended, when its turn started, its open approvals, background tasks, prompts not taken, whether its queue is held, and whether its agent opened | Worked out from its log, in the same transaction as the event | `sessions` | The hub's level |
| What the run that's going claims it can do | Worked out from its log too: said in the event that starts the run, and taken off with the one that ends it | `sessions.agent_capabilities` | The hub's level |
| A session's subagents | Worked out from its log, in the same transaction as the event | `subagents` | A session's level, listed and then each change |
| A session's waits on pull requests: what each waits for, what it saw as it started, its deadline, how it ended, and, once it has, what it last read, frozen | Asked for by an agent, through its tools | `waits` | The hub's level, on the session's record |
| Proposals, each revision, and the answers to them | What agents and people entered through Roundhouse | `proposals` and `proposal_revisions` | A session's level, listed and then each change, and `ProposalRevisionsRead` |
| Drafts: a session's composer and its answer to an approval, a comment on a workspace's diff, and the new workspace with its files | What a person typed | `sessions.draft`, `sessions.decision_draft`, `workspaces.comment_draft`, `new_workspace_draft`, `new_workspace_draft_files` and the files | The hub's level |
| Comments on a workspace's diff | What a person entered | `diff_comments` | The hub's level |
| How far each chat is read, and when it and its workspace were last worked in | One person's, kept for them | `sessions.read_seq`, and the `user_active_at_ms` of `sessions` and `workspaces` | The hub's level |
| In conversation mode, which device has each chat's conversation, and how each reply read out ended | One person's, kept for them | `sessions.conversation` and `replies_heard` | The hub's level, and a session's for the replies |
| Hub settings, each agent kind's default account, and whether Automatic puts its chats on accounts | Chosen by a person | `hub_settings` and `account_defaults` | The hub's level, on the settings' record |
| The default GitHub account, what the hub sends workers for it, and the providers it sets GitHub up on or leaves it to | Chosen by a person, or worked out by the hub from its logins and its providers file | `hub_settings` | The hub's level, on the settings' record |
| Agents' accounts: the name a person gave one, its held login, its reserve and when it was last used | Roundhouse's, and what it holds | `accounts`, and the login files | The hub's level |
| GitHub accounts: the GitHub user each is, by its ID, its held login, and the app installation it is | Roundhouse's, and what it holds | `github_accounts`, and the login files | The hub's level |
| Apps of your own, by their ID, with their keys and what each was made from | Roundhouse made each, through GitHub | `github_apps`, and the key files | The hub's level |
| Sign-ins with GitHub | Roundhouse's, so every client shows the same code | `github_sign_ins` | The hub's level |
| Tool servers: name, URL and how each knows the person | A person added each | `tool_servers`, and the files of their keys and sign-ins | The hub's level |
| Paired devices: name, when paired and when last seen | Roundhouse's, and what it holds | `devices` | `DeviceList` |
| A workspace's open terminals | A person opened each. Kept while its shell runs | The hub's memory | The hub's level |
| Whether the hub is asking an agent which models it offers, and how that went | The hub's own work, which lasts only as long as the hub | The hub's memory, on the list of models' `ModelList` record | The hub's level, and `ModelsFor` |
| How an update a person asked for is going, on a hub installed as a service, and when the hub last read the update feed and why that failed | The hub's own work, which lasts only as long as the hub | The hub's memory, on its `HubUpdate` record | The hub's level |

### Observations

*Not built:* any observation but a workspace's lines added and removed, its changed files, its branches, which build its worker is, the release a hub would update to, what GitHub says of a chip's issue or pull request and of a workspace's own, whether the default GitHub account can push to a repository, what GitHub calls a GitHub account's user, what GitHub says of an app of your own and where each of its installations is, what an agent offers, who an agent's account is, an account's usage limits, how full an agent's context is, its todos and its tool servers, what the API's cache holds of a conversation, and what each open wait last read, so each of the others is kept as its row says: as a record or a field of one, in GitHub's cache, which reaches a client with no revision, or not at all.

| What | Whose | Kept in | Sent by |
|---|---|---|---|
| A workspace's lines added and removed | Git's | The hub's memory and `observed_workspace_lines`, each with when it was read. The hub reads them from the worker as a turn ends in the workspace and as its branches move, and while a client follows the workspace its worker, if it's running, watches the worktree and sends them as they change. Forgotten as the workspace is archived | The hub's level, as an observation, listed and then each change |
| A workspace's changed files, each with its lines and version | Git's | The hub's memory and `observed_workspace_changes`, each with when it was read, from the same read as the lines, and current with them while watched. Forgotten as the workspace is archived. Not kept: the files past a summary's 5,000, and the diff. The worker reads those, and one file's diff, in pages, and the whole diff at once | The workspace's level, as an observation, listed and then each change. `WorkspaceChangedFiles`, `WorkspaceDiff` and `WorkspaceFileDiff` |
| How many files a workspace has uncommitted | Git's, as the workspace's worker counts them for a read of its pull request someone asked for | In the observation of how the workspace's pull request stands, below, as of the read that counted them | With that observation |
| A workspace's branches, and each of its other worktrees' | Git's, as the workspace's worker watches them | The hub's memory, and `observed_workspace_branches` for a workspace's own, current while its worker watches them and as of when it went otherwise, and forgotten as the workspace is archived. `observed_child_worktree_branches` for another worktree's, forgotten with the workspace | The workspace's level, for its own, as an observation. Another worktree's reaches no client |
| Which build a workspace's worker is, while it's another than the hub's | The worker's, as its hello says, which the hub hears of each worker an earlier hub left that it carries on with | The hub's memory alone, current while that worker is connected, and gone once it isn't, and as the hub starts | The hub's level, as an observation |
| What GitHub says of an issue or a pull request: its summary, checks, stack, when a review last asked for changes and when it last left the merge queue, and its branch's rules for checks | GitHub's | The hub's memory and `facets`, each facet with when it was read, under one version | An issue's or pull request's level, and the hub's for a workspace's own, with no revision, and `ReferencesRead` and `WorkspacePullRequest`. The rules only say which checks are required |
| What GitHub says of an issue or a pull request, as a chip draws it | Worked out from GitHub's | The hub's memory and `observed_referenced`, worked out from `facets` while a client follows it, and gone once `facets` forgets it | An issue's or pull request's level, as an observation |
| How a workspace's pull request stands: the pull request, its summary, and how far its worktree is ahead and behind | Worked out from git's and GitHub's | The hub's memory and `observed_workspace_pull_request`, worked out from the workspace's record, its branches and `facets` as any changes | The hub's level, as an observation |
| A workspace's pull request's checks, history and stack, of the head they were read at | GitHub's | The hub's memory and `observed_workspace_pull_request_detail`, worked out as the row above | The workspace's level, as an observation |
| That GitHub showed the default GitHub account no repository where a workspace's pull request would be | GitHub's | The hub's memory and `observed_workspace_repository_unseen`, for a workspace it ever showed none, as of the read that heard it. A read that fails for another reason leaves it | The workspace's level, as an observation |
| The pull requests each open wait watches, as last read | GitHub's, worked out from GitHub's part of the cache as each read of them ends | The hub's memory and `observed_wait_read`, as of the read. Forgotten as the wait ends, when its record takes it, and with its session | The session's level, as an observation, and in each answer to the agent |
| Whether the default GitHub account can push to a repository | GitHub's, as the hub checks with the default's login as it starts, as a worker reads the repository's address, as the default or its login changes, and each half hour, or what the hub says without asking, as for one not on GitHub | The hub's memory and `observed_repository_reach`, as of GitHub's answer. Forgotten with the repository | The hub's level, as an observation |
| A GitHub account's username, name and email | GitHub's, as the hub asks with the account's login as it's connected, daily after, and at once when it doesn't know them, or with no login, by the user's number, where that login can't be used, and a day on once GitHub says it has no such user, or, for an app's installation, the app's bot, as its record says | The hub's memory, `observed_github_username` for the username and `observed_github_profile` for the name and email, as of the hub's read. Forgotten with the account | The hub's level, for the username, as an observation. The name and email reach no client, and go to workers as git's author for commits |
| What GitHub calls an app of your own, its owner and its bot, and the user or organization an installation is on | GitHub's, as it says them as it makes the app, and as the app asks it with its key, as the hub starts, each day after and at once when the hub doesn't know them, its bot asked by the app's slug, and where each installation is as each look lists them, each minute | The hub's memory, `observed_github_app_said`, `observed_github_app_bot` and `observed_installed_on`, as of each read. Forgotten with the app, and with the installation's account. The number of what an installation is on stays on its account's row, `github_accounts.installed_on_id`, as what links it to GitHub's installation | The hub's level, as observations |
| What an agent offers where a chat would launch, by its list of models' key | The agent's CLI | The hub's memory and `observed_model_catalog`, as of the agent's answer. Let go of with its list, as which login a launch is handed changes | The hub's level, as an observation, and `ModelsFor` |
| Who an agent's account is: its email, plan, model provider and how it's logged in | The provider's, read from the agent's login by a worker after each launch on a login on its machine | The hub's memory and `observed_account_identity`, as of the worker's read. Forgotten with its account | The hub's level, as an observation |
| An account's usage limits | The provider's, said by an agent in a report, which says only some at a time | The hub's memory and `observed_account_limits`, each limit with when it was heard, merged with what's kept as each report comes, and as of the newest. Forgotten with its account | The hub's level, as an observation |
| How full an agent's context is, and its todos | The agent's, said in a report of the session's current run | The hub's memory, and `observed_session_context` and `observed_session_todos`, as of the report. Forgotten with its session | The session's level, as an observation |
| The tool servers an agent has, but for the worker's own | The agent's, said in a report of the session's current run | The hub's memory, and `observed_session_tool_servers`, as of the report. Forgotten as the run ends, as another starts, and with its session | The session's level, as an observation |
| What the API's cache holds of a conversation | The provider's, said by an agent of each of its requests, and told as a request is made while the agent isn't working, as a turn ends, and as something empties it | The hub's memory, and `observed_session_cache`, as of the request, with whether the context is big enough to tell of. Forgotten with its session | The hub's level, as an observation |
| The release a hub installed as a service would update to | The update feed's, as the hub reads it as it starts and every 10 minutes after | The hub's memory alone, current while its reads of the feed work, with the archive it updates from beside it | The hub's level, as an observation |
| GitHub's limit on each token | GitHub's | The hub's memory | No client |

### Read on request, and not kept

The hub reads these from a worker, or from its own machine, each time a client asks, and keeps nothing of them: one file's diff and the whole diff, which only the command line reads, the file list, a file's lines, a folder, the setup log, git's status for an archive, a terminal's rows and text, and who `gh` and the SSH key on the hub's machine are. It answers from its memory with its status (`HubStatus`), what each kind of agent it runs offers (`AgentList`), and a code to pair a device, which it keeps only until the code expires.

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
| Where each login, key and device token is | `login_file` of `accounts` and `github_accounts`, `github_apps.key_file` and `made_from`, `tool_servers.secret_file`, and `devices.token_sha256` |
| The sign-ins to tool servers in progress, and how the last renewal of each went | The hub's memory |
| What workers are sent for the tool servers, with their keys and access tokens, which each worker has in memory too | The hub's memory, worked out from `tool_servers` and the files of their keys and sign-ins |
| Which sign-in each run has had each of its tool servers by, and which of them it has been asked to connect to again and hasn't answered for | The hub's memory |
| Which sign-ins to tool servers each worker has the tokens of | The hub's memory, on the worker's link, from the sets it has applied |
| What each client shows of GitHub, and when the hub reads GitHub next | The hub's memory |
