# Architecture

How the system fits together. What's built so far is the local path: the protocol, hub, worker, Claude adapter, local provider, CLI, transcript and desktop app. Everything beyond that is the agreed direction, and still a proposal. Decisions are in `docs/decisions.md`, and the build order is in the [GitHub milestones](https://github.com/zlangbert/trestle/milestones).

## Core model

- **A hub owns state and serves the platform API.** It holds workspaces, sessions and event logs. Clients only talk to hubs, and a client can connect to several at once and merge them into one sidebar.
- **A worker does the work.** It runs agents, terminals and git for a workspace, and reports events to its hub.
- **A provider gives a hub somewhere to run workers.** Running on the same Mac is one provider, an Incus instance is another, and a cloud sandbox is another. No provider is special, and **any hub can use any provider**.

```
Desktop, iPhone, CLI
   │ client protocol
   ▼
Hub: on a Mac, on your own server, or a hosted service
   │ provider: local, Incus, cloud sandbox, or your own adapter
   │ worker protocol, over any byte stream
   ▼
Worker: on the same Mac, in an Incus instance, or in a sandbox
```

## Where a hub can run

All of these are the same code and the same API. A workspace belongs to one hub.

| Hub | Runs on | Serves | Storage | Auth |
|---|---|---|---|---|
| **Local** | A Mac | One person | SQLite | A Unix socket only this user can open. The relay path needs its own |
| **Self-hosted** | Your own server | You or your team | SQLite, Postgres if it grows | Accounts or tokens |
| **Hosted**, if one is ever run | A shared service | Many users | Postgres | Accounts |

Any of them can use any provider. A hub is only reachable while its machine is awake, so persistent mobile access needs an always-on hub.

## Platform API

The hub serves this API to the desktop app, mobile app, CLI and automation:

- **Workspaces:** create, list, archive and restore.
- **Sessions:** read history by range, subscribe to new events, send prompts and approvals, and cancel.
- **Per-workspace data:** diff comments, unread, pinned and drafts.
- **Live access:** attach to a terminal, and get diffs, git status and PR status. The hub fetches or proxies these from the worker.

The hub doesn't do the work itself. When a workspace is created, it records it and tells a worker to create the worktree and start the agent. The worker reports events back, and the hub saves them and passes them to clients.

## Alternatives considered

- **The same full daemon everywhere,** with each sandbox owning its log and the control plane holding a copy. Simpler to start, but it means two copies of each cloud log and a second read path for sleeping sandboxes.
- **Separate local and cloud paths.** It would duplicate features.
- **Everything through a central sync engine.** It would break offline use and keeping local data on the device.

## Layout

A single Cargo workspace in one repo. Items marked **built** exist now, and only as deep as the product needs so far.

```
crates/
  protocol/        Built. Client and worker protocol types, event log schema, versioning. No IO
  hub/             Built. Workspaces, sessions, event log, platform API, SQLite storage
  worker/          Built: git, worktrees, running agents. Later: terminals, buffered outbox
  agents/          Built: Claude adapter, raw event capture. Later: Codex
  process/         Built. Signalling a process group, and recording a PID with its start time
  providers/       Built: provider contract, local provider. Later: external adapter host
  client-core/     Built: connection, subscriptions, a window of a session's log. No UI
  transcript/      Built. A session's history and live output, on the kit's list and text views
  terminal-view/   GPUI terminal element
apps/
  daemon/          Built. One binary that runs as a hub or a worker
  cli/             Built. Command-line client
  desktop/         Built: GPUI and gpui-kit app, with its UI tests. Later: terminals, tabs, settings
  mobile-ios/      Added after the proof of concept
providers/
  incus/           First external adapter
services/
  relay/           Lets phones reach hubs that aren't publicly reachable
  hosted/          Accounts and billing, only if a hosted service is run
third_party/
  option-ext/      Built. Stands in for an MPL-2.0 crate that would otherwise ship. See its source
tools/
  policy/          Built. Tests that enforce the license and dependency rules below
  smoke/           Built. Runs the stack against the real Claude CLI, and regenerates its fixture
  perf/            Performance tests against the real app. Not built yet, see `docs/performance.md`
```

The binaries are `trestle`, `trestled` and `trestle-desktop`. `crates/protocol/src/paths.rs` names the data directory and the variables that move it.

## Dependency rules

```
apps/desktop ──> transcript, terminal-view ──> client-core ──> protocol
apps/cli ─────────────────────────────────────> client-core
apps/daemon ──> hub ──> providers, protocol
apps/daemon ──> worker ──> agents ──> protocol
agents, providers, worker ──> process
```

1. **Clients only talk to hubs.** They never touch git, terminals or agents directly. The hub fetches or proxies live data from the worker.
2. **`protocol` has no IO and no UI.** Everything depends on it, so it needs to stay small and stable.
3. **The hub and worker must build on Linux without GPUI.** Nothing under `hub`, `worker`, `agents` or `providers` may depend on GPUI or gpui-kit.
4. **GPUI stays in `transcript`, `terminal-view` and `apps/`.** `client-core` must stay free of it, so that a native mobile client remains possible if the GPUI path fails.
5. **The worker boundary is real from day one.** On a Mac, the worker still runs as a separate process, so remote workers only change the transport.
6. **Exact pins for GPUI and gpui-kit.** They are pre-1.0, so upgrades should be deliberate and in one place.

`tools/policy/tests/policy.rs` checks rules 1 to 4 and the license rule against the real dependency graph, so `cargo test` fails if a new dependency breaks one. The license check covers what actually ships on each platform, not everything in `Cargo.lock`. Rule 3 was also checked by building and testing on Linux in Docker.

`cargo test` covers everything that builds without GPUI, which is what a Linux server needs. `cargo test --workspace` adds the transcript and desktop app.

## State ownership

Each piece of state has one owner, and every other copy is a cache.

| State | Examples | Owner |
|---|---|---|
| Session history | Agent events, prompts, approvals | The workspace's hub |
| Workspace records | Branch, status, notes, diff comments | The workspace's hub |
| Per-user workspace state | Unread, pinned, drafts | The workspace's hub, stored per user |
| Live processes | Running agents, terminals, run scripts | The worker |
| Filesystem | Uncommitted files, dependencies, agent session files | The worker's machine, kept across sleep by the provider |
| Code, PRs, CI | Git and GitHub data | Git and GitHub. The worker and hub only cache |
| Per-device view | Scroll position, open panels, selection | The client |

- A session has one writer, so events get a simple sequence number and no merge logic is needed.
- Clients read a range, then subscribe from an offset. Reconnecting means catching up from the last offset.
- A prompt or approval only becomes real once it's recorded. Until then, the UI shows it as pending.
- **A running agent decides its own model and approval mode.** A change goes to the agent first and is saved only once it agrees, and the agent reports every change of mode itself, so what's shown is never ahead of what's true.

### Wire format and event log

Settled in step 1.1. Details are in `crates/protocol/`.

- **Newline-delimited JSON over any byte stream,** which is a Unix socket for clients and stdio for workers. It works over anything, it can be read by hand, and agent output is already JSON.
- **Versioning:** both sides say hello first and compare a protocol number. Unknown fields are ignored, and unknown enum variants decode as `Unknown`, so additions don't need a version bump. Four enums don't do this yet, see #51.
- **The database only migrates forward.** Migrations run in order, each in its own transaction, and a database from a newer build is refused rather than damaged.
- **Each event has a normalized body, plus the agent's raw line in a separate column.** Clients read the normalized form. The raw line means views can be rebuilt if normalization was wrong, and range reads don't load it.
- **Live deltas aren't stored.** The finished message arrives as a durable event and replaces them. A client attaching mid-message gets a snapshot of the streaming tail, then deltas, and `protocol::tail` holds the rules so the hub and clients can't disagree.
- **Subscribers read from the log, and live updates only say that it moved.** A slow client can fall behind and catch up without losing anything, and it holds back only itself.
- **A status change commits in the same transaction as its event,** so a client reading status after an event can't see an older value.
- **Prompts carry an optional `client_msg_id`,** and the same ID is only recorded once, so a client can retry safely after a disconnect.

## When a process dies

Any process can die at any point, so nothing depends on a clean exit. Whatever survives works out what happened from the records.

- **A hub is a separate process that outlives its clients,** so closing the app doesn't stop its agents. A client connects, or starts a hub if there's none, and a lock keeps each data directory to one hub.
- **A prompt is recorded before anything else is tried,** and the client gets its reply at that point. Later failures go in the log, where every client can see them.
- **A hub cleans up at startup.** Its workers exited with it, so it closes open approvals, marks active sessions `Interrupted`, fails setups that were cut off, and discards half-made workspaces.
- **The worker buffers events on disk until the hub acknowledges them.** A worker has to keep going while its hub is unreachable, such as when a Mac running the hub goes to sleep. Not built yet: it comes with milestone 2. Until then a worker stops its agents when it loses its hub, so nothing runs unrecorded, and the next prompt resumes the conversation.
- **When a worker goes, the hub stops its instance and tells its sessions,** which close their open approvals and record that the agent exited. The next prompt starts a new worker and agent.
- **A worker that gets killed can't stop its agents, so it records them.** Each agent's PID and start time go in a file in the instance directory, and the local provider stops anything still recorded before the next worker starts. Otherwise an orphan and its replacement would both be working on one conversation and worktree.
- **Each launch of an agent has a run ID,** and its events carry it. A late event from a replaced agent still goes in the log, but can't change the status of the session its replacement is running.

## Providers

The hub needs a small contract from a provider:

| Operation | Purpose | Required |
|---|---|---|
| `create` | Make an instance and place the worker binary in it | Yes |
| `start`, `stop` | Wake and sleep. Stopping also ends anything the worker left behind | Yes |
| `kill` | Like `stop`, without giving a worker that isn't responding time to finish | Yes |
| `connect` | Return a byte stream to the worker | Yes |
| `destroy` | Remove the instance | Yes |
| `snapshot`, `restore` | Preserve or roll back the filesystem | Optional, declared by the provider |

- **Workers only need a byte stream.** An adapter can use `incus exec` over stdio, SSH, a WebSocket or anything else. The hub dials out through the adapter, so workers don't need to reach the hub.
- **External adapters are separate executables, over stdio to start.** The hub runs them and talks to them over stdin and stdout, so they can be written in any language without rebuilding the app. The built-in local provider implements the same contract. A gRPC API may come later, so the contract should be defined as messages, independent of the transport.
- **Providers declare their capabilities,** such as whether the filesystem survives a stop and whether they support snapshots. Persistent instances don't need snapshots to sleep, while ephemeral sandboxes do.
- **Availability follows the hub.** A hub on an intermittently available machine, such as a Mac, is only reachable while that machine is awake. Agents on remote workers keep running and catch up later. Persistent mobile access needs a hub on an always-on machine.
- **Each workspace gets its own worker process and provider instance,** so a crash in one doesn't touch the others. This matches sandboxes, where each workspace has its own machine. A worker uses about 8 MB, so this is cheap on a Mac.
- **The contract is a Rust trait for now.** It becomes messages when the first external adapter needs them, in milestone 2. `snapshot` and `restore` come with the first provider that uses them.
