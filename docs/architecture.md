# Architecture

How the system fits together, and why. This is the agreed design, so it includes parts that aren't built, and those are marked. Where the code does something simpler for now, this says what.

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
- **An off-the-shelf sync engine,** such as PowerSync, Electric or Turso's sync. They solve merging writes from clients that work offline, which a hub that's the only writer doesn't need. They need Postgres or copy a whole database to each client, and none covers a window of a log or a tail that isn't stored, while "Keeping clients in sync" needs only a revision and a few rules. Clients that write while offline would be the reason to look again.
- **CRDTs for session history.** They suit many people typing in one buffer, which isn't this product. Long sessions are remote, so clients read a log by range instead, and never hold a whole transcript.

## Layout

A single Cargo workspace in one repo. Directories marked *not built* don't exist.

```
crates/
  protocol/        Client and worker protocol types, event log schema, versioning. No IO
  hub/             Workspaces, sessions, event log, platform API, SQLite storage
  worker/          Git, worktrees, running agents
  agents/          Claude adapter, raw event capture
  process/         Signalling a process group, and recording a PID with its start time
  providers/       Provider contract, local provider
  client-core/     Connection, and each hub's records and open sessions, kept in sync. Shared by every Rust client. No UI
  transcript/      A session's history and live output, on the kit's list and text views
  look/            How the app looks: its theme, the colours the kit has no name for, the status mark and the app icon
  terminal-view/   GPUI terminal element. Not built
apps/
  daemon/          One binary that runs as a hub or a worker
  cli/             Command-line client
  desktop/         GPUI and gpui-kit app, with its UI tests
  mobile-ios/      iOS app, after the proof of concept. Not built
providers/
  incus/           First external adapter. Not built
services/
  relay/           Lets phones reach hubs that aren't publicly reachable. Not built
  hosted/          Accounts and billing, only if a hosted service is run. Not built
tools/
  policy/          Tests that enforce the license and dependency rules below
  smoke/           Runs the stack against the real Claude CLI, and regenerates its fixture
  install.sh       Installs this commit as a version of its own, outside `target/`
  perf/            Performance tests against the real app. Not built, see `docs/performance.md`
.github/workflows/ CI: the full checks on Linux for every PR that changes more than Markdown, and on a Mac when dependencies change
```

The binaries are `roundhouse`, `roundhoused` and `roundhouse-desktop`. `crates/protocol/src/paths.rs` names the data directory and the variables that move it. Only a release build defaults to the real data. A debug build uses `roundhouse-dev`, `cargo run` and `cargo test` use a directory in the checkout, and nothing a worker starts inherits these variables, so what's built in a workspace can't reach the hub it's running under.

## Dependency rules

```
apps/desktop ──> transcript, terminal-view ──> client-core ──> protocol
apps/desktop, transcript ──> look
apps/cli ─────────────────────────────────────> client-core
apps/daemon ──> hub ──> providers ──> protocol
apps/daemon ──> worker ──> agents ──> protocol
agents, providers, worker ──> process
```

1. **Clients only talk to hubs.** They never touch git, terminals or agents directly. The hub fetches or proxies live data from the worker.
2. **`protocol` has no IO and no UI.** Everything depends on it, so it needs to stay small and stable.
3. **The hub and worker must build on Linux without GPUI.** Nothing under `hub`, `worker`, `agents` or `providers` may depend on GPUI or gpui-kit.
4. **GPUI stays in `transcript`, `terminal-view`, `look` and `apps/`.** `client-core` must stay free of it, and of any async runtime: it's blocking calls and channels, so GPUI's executor stays out, and a native mobile client remains possible if the GPUI path fails. `look` is a crate of its own because both the app and `transcript` draw with it, and `transcript` can't depend on the app. *Unconfirmed.*
5. **The worker boundary is real.** Even on a Mac, the worker runs as a separate process, so remote workers only change the transport.
6. **Exact pins for GPUI and gpui-kit.** They are pre-1.0, so upgrades should be deliberate and in one place.
7. **Nothing that ships is under GPL, AGPL or LGPL.** MPL-2.0 is allowed when it comes unchanged from crates.io, and everything else is permissive, such as Apache-2.0 or MIT. GPL or AGPL code would put the whole app under its terms, which would decide the project's own license and rule out closed or hosted versions, and GPL is widely seen as incompatible with the App Store. AGPL also reaches the hub over the network. LGPL expects users to be able to relink, which static linking makes awkward. MPL-2.0 only covers its own files (§3.3), and shipping it unchanged only means saying where its source is (§3.2), which goes in the notices every license needs anyway. A changed copy would have to be published, hence crates.io only.

`tools/policy/tests/policy.rs` checks rules 1 to 4 and 7 against the real dependency graph, so `cargo test` fails if a new dependency breaks one. The license check covers what actually ships on each platform, not everything in `Cargo.lock`.

### Layers

Dependencies point inward, from handlers to operations to stores and integrations. A layer answers with a lower layer's type wherever it means the same, rather than copying it.

| Layer | What it does | Here |
|---|---|---|
| Handlers | Parse a request, call one operation, map its result | `hub::server`, `worker::serve` |
| Operations | One module per area, holding its logic | `hub` |
| Stores | All SQL, inline: a few dozen statements, with nothing to check them at compile time | `hub::store` |
| Integrations | One module per outside system. Nothing else knows its flags, wire format or retries | `agents::claude`, `worker::git`, `hub::worker_link`, `providers::local` |
| Edges | Configuration and telemetry, read from the environment and passed down | `main` in each app |

Errors follow the layers:

- **Hub and worker operations return `protocol::client::ApiError`,** the error both protocols carry. Its `ErrorCode` is a closed set of meanings, such as `NotFound` and `Conflict`, not a transport status, so a second enum mapping onto it would only be a copy. Callers and tests match `code`, and `message` is for people. A condition callers must tell apart gets a new `ErrorCode`.
- **`hub::store` has its own enum,** because `ApiError` holds only text, and using it at the bottom would throw away the SQLite error.
- **Lower errors become `ApiError` in one place per crate,** an `impl From<…> for ApiError`, so the mapping reads as one table. `Internal` failures get logged there, with their source.

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
- A prompt or approval only becomes real once it's recorded. Until then, the UI shows it as pending.
- **A running agent decides its own model and approval mode.** A change goes to the agent first and is saved only once it agrees, and the agent reports every change of mode itself, so what's shown is never ahead of what's true.

## Keeping clients in sync

A client's copy of a hub's state has to stay correct through lag, reconnects and restarts at either end, without each view working out how. Everything a hub serves is one of three kinds, and each kind has one way to tell a newer copy from an older one, and one way to recover.

| Kind | What | Newer or older | Recovering |
|---|---|---|---|
| Log | A session's events | By `seq`, which has no gaps | Subscribe again from one past the last `seq` |
| Record | Repositories, workspaces, sessions, comments, and per-user state | By revision, `rev`, from one counter per hub that moves in the same transaction as the change | List again, and keep whichever copy has the newer revision |
| Live | Output that isn't stored, such as the tail | Nothing to compare. It belongs to one run, and arrives in order on one connection | Replace it with a new snapshot |

- **For logs and records, which copy is newer is decided per item, so the order things arrive in doesn't matter.** A duplicate or an older copy is dropped by its `seq` or revision, so a client doesn't depend on how a list and a feed notice interleave, which a relay or a second connection could change. A list is read in one transaction and carries the revision it was read at, so a record missing from it is gone, unless the client holds a newer copy from the feed. Removing a record is a change like any other, with a revision of its own. A session's newest event isn't announced, so it isn't part of its revision, and the log says how far a session has got.
- **Only the current run's live output reaches clients.** The hub knows which run it's starting before it launches it, and drops live output from any other, so a replaced agent can't clear or add to its replacement's tail. A replaced run's events are history, so they go in the log as usual, as "When a process dies" says. A run's ID doesn't repeat within its session, even across hub restarts, so this holds once workers outlive their hub. Each event and delta carries its run, and the end of a turn or of an agent clears what its run or an earlier one left open, never a later run's, so a client that sees a replaced run's last events keeps the same tail as the hub. A later launch always has a higher run ID.

**Every ID has one allocator, and is unique only within its scope,** so nothing is taken as unique where it might not be.

| ID | Chosen by | Unique within |
|---|---|---|
| Repository, workspace, session, comment | The hub, as a UUID | Everything, so one sidebar can merge several hubs |
| `seq`, run | The hub | A session |
| Revision | The hub | A hub |
| `client_msg_id` | A client | A session |
| `client_request_id` | A client | A hub |
| Subscription | A client | A connection |
| Message, tool use, approval | The agent | A session. Nothing promises an agent won't reuse one elsewhere, so they're never keys outside it |

- **An item keeps one identity through every state it passes through.** A prompt is its `client_msg_id`, from pending to recorded. A block of the agent's output is its message ID and index, from its first delta to the event that records it. Anything with no identity before it's recorded is its `seq`, and its position in that event. The transcript makes the element IDs of what each item shows from these in one function, so nothing on screen resets when a prompt is recorded or a block finishes. Its buttons belong to one event and never change state, so they're keyed by that event.
- **A command is safe to repeat, and its reply says only what the hub has recorded.** Each request that changes something either has the same effect however often it's applied, such as closing a session, or carries an ID the client chose, as a prompt carries `client_msg_id`. Whatever happens after the reply reaches every client through the log and records, not through the reply. Each request in `protocol::client` says which kind it is. *Not built:* cancelling a turn and running setup again carry no such ID, so sent again later, they act again.
- **client-core keeps each hub's copy, and views only read it.** For each hub a client is connected to, client-core holds the records and a window of each open session, and does all the subscribing, fetching and recovering, reconnects included. It applies updates as they arrive and tells a view what changed, combining whatever the view hasn't looked at, so a slow view sees the latest state rather than a backlog, and nothing queues without limit. Views keep only per-device state, such as scroll position and selection, keyed by the IDs above, so it survives a reconnect. That way the CLI, the desktop app and a native mobile app make the same guarantees without each implementing them. *Not built:* the desktop app makes the new connection itself, since it knows how to start its hub, and hands it to client-core.

## Wire format and event log

Details are in `crates/protocol/`.

- **Newline-delimited JSON over any byte stream,** which is a Unix socket for clients and stdio for workers. It works over anything, it can be read by hand, and agent output is already JSON.
- **Versioning:** both sides say hello first and compare a protocol number. "Changing the protocol" says what needs a bump, which is rare.
- **`hub_status` and `hub_shutdown` work without a hello,** so a client of any version can see a hub and stop it. That's how `roundhouse hub status`, `roundhouse hub stop` and the app's `Restart the hub` work across a real break, and it's why the app can say how many agents would stop. Anyone who can reach the socket is this user. A connection from elsewhere, once there are any, shouldn't get this.
- **The database only migrates forward.** Migrations run in order, each in its own transaction, and a database from a newer build is refused rather than damaged.
- **Each event has a normalized body, plus the agent's raw line in a separate column.** Agent output is untrusted input, so `agents::claude::normalize` takes any line without panicking. Clients read the normalized form. The raw line means views can be rebuilt if normalization was wrong, and range reads don't load it.
- **Live deltas aren't stored.** The finished message arrives as a durable event and replaces them. A client attaching mid-message gets a snapshot of the streaming tail, then deltas, and `protocol::tail` holds the rules so the hub and clients can't disagree.
- **Subscribers read from the log, and live updates only say that it moved.** A slow client can fall behind and catch up without losing anything, and it holds back only itself.
- **A status change commits in the same transaction as its event,** so a client reading status after an event can't see an older value.
- **Prompts carry an optional `client_msg_id`,** and the same ID is only recorded once, so a client can retry safely after a disconnect.

### Changing the protocol

These rules let peers on different versions keep working together.

- **Each shape has one form.** It's the serde form in `protocol`, on the wire and in the hub's SQLite columns, so changing a type also changes how old rows read.
- **A new field is `Option` or `#[serde(default)]`,** so a message from an older peer still decodes. The same goes for old rows.
- **Every enum read as data has `#[serde(other)] Unknown`:** statuses, kinds, events and blocks. Without it, one new value fails the whole frame, which `client-core` drops, failing the call if it was a reply. Code that receives `Unknown` skips it or shows something neutral. It never panics.
- **Other envelopes have no `Unknown`.** That covers `HubFrame`, `ClientResponse` and the like, since there's nothing sensible to do with one. A client only gets a new response by making a new request.
- **A new request doesn't need a bump, and whoever calls it has to expect a refusal.** A hub answers a request it doesn't know with `ErrorCode::Unsupported`, and carries on. A client checks `is_unsupported()`, and hides or disables whatever needed it. There's no list of capabilities until something needs one.
- **A version bump means a real break,** meaning a change that a peer on the last version can't read under these rules. Client and hub can't work together across one, so it should be rare.
- **`hub_status`, `hub_shutdown` and refusals can't change form, even in a real break.** They're what lets a client see a hub and stop it across one. `crates/protocol/tests/compatibility.rs` spells them out, so changing one fails it.
- **The worker protocol follows the same rules once workers can differ in version.** Today a hub and its worker are one binary.

## When a process dies

Any process can die at any point, so nothing depends on a clean exit. Whatever survives works out what happened from the records.

- **A hub is a separate process that outlives its clients,** so closing the app doesn't stop its agents. A client connects, or starts a hub if there's none, and a lock keeps each data directory to one hub.
- **A prompt is recorded before anything else is tried,** and the client gets its reply at that point. Later failures go in the log, where every client can see them.
- **A hub cleans up at startup.** Its workers exited with it, so it closes open approvals, marks active sessions `Interrupted`, fails setups that were cut off, and discards half-made workspaces.
- **The worker buffers events on disk until the hub acknowledges them.** A worker has to keep going while its hub is unreachable, such as when a Mac running the hub goes to sleep. For now, a worker stops its agents when it loses its hub, so nothing runs unrecorded, and the next prompt resumes the conversation.
- **When a worker goes, the hub stops its instance and tells its sessions,** which close their open approvals and record that the agent exited. The next prompt starts a new worker and agent.
- **Workers, agents and setup scripts each start in their own process group,** so stopping one reaches everything it started, and a signal to the hub's terminal doesn't reach them directly. Signalling a group, and recording a PID with its start time, each have one implementation, in `crates/process`.
- **A worker that gets killed can't stop its agents, so it records them.** Each agent's PID and start time go in a file in the instance directory, and the local provider stops anything still recorded before the next worker starts. Otherwise an orphan and its replacement would both be working on one conversation and worktree.
- **Each launch of an agent has a run ID,** and its events carry it. A late event from a replaced agent still goes in the log, but can't change the status of the session its replacement is running, or clear its replacement's tail.

## Managing a hub

Who starts, updates and stops a hub is a separate role from the hub itself, and what fills it depends on where the hub runs. For now, the app replaces a local hub only when it connects, not while it stays open.

| | Local hub started on demand | A hub you run yourself, here or on a server | Hosted, if one is ever run |
|---|---|---|---|
| **Starts it** | Whichever client needs it | You, systemd or Docker | The service |
| **Updates it** | The desktop app, when nothing is running | You or your package manager | The service |
| **When versions differ** | Keeps working, and the app replaces the hub later | Keeps working, and the app says the hub is older | Keeps working |

- **A hub knows nothing about what manages it.** It offers its status and stops cleanly on `SIGTERM`, which is what systemd and Docker use. There's no desktop mode, and it doesn't exit when the app closes.
- **A client never needs one exact hub version.** It hides or disables what an older hub doesn't support, as "Changing the protocol" describes, and only a real break stops it. This stays as simple as it can, and gets revisited if it turns out complicated.
- **A client only replaces a hub that says it's replaceable.** A hub started on demand reports that in its status, and one you started yourself gets left alone, even on the same Mac. Only the desktop app replaces a hub, and only an older one, so two versions can't keep replacing each other's.
- **Only a local on-demand hub that's older than the client can be restarted from it.** The app offers that whenever such a hub is older, so nobody has to wait for it to be idle, and only a real break puts it in the way. A client's own hub might not be able to open what a newer one has written. A remote mismatch only gets an explanation. Stopping a shared hub is for its admins, not for any client whose version differs.
- **The app ships a default hub, and that's the only link between them.** The hub doesn't depend on the app.
- **`tools/install.sh` copies release binaries into `roundhouse-builds/<version>/`,** and a `current` link names the one to start. Clients and hubs resolve the link when they start, so they stay on their version when it moves. Running from `target/` would give a rebuilt hub workers from different code, and archiving that worktree would remove the binary. Keeping versions gives rollback by moving the link.
- **An install stamps its commit through `ROUNDHOUSE_BUILD`,** and any other build says `0.1.0-dev`. The app and `roundhouse hub status` show it, and say so when the app and hub differ, because a bug report has to say which code it came from. A build script would rebuild the crates above it on every commit, for builds nobody needs to identify.
- **An install also stamps an order through `ROUNDHOUSE_BUILD_NUMBER`:** when its commit was made, or when it installed if it has uncommitted changes. Any other build has no number, so it's never replaced. Replacing only an older hub needs an order, and a commit hash has none. Install time would let installing an older commit replace a newer hub, whose database it might not be able to open, and commit counts disagree across branches. Rolling back keeps its older number, so the app leaves the newer hub alone until it's stopped.

What someone using the app sees during an update is in `docs/spec/updates.md`. Workers that outlive their hub would let the app replace a hub without waiting for it to be idle, as "When a process dies" describes.

### Updating the app

The app is downloaded, signed and notarized, and it updates itself from inside the app as changes land on `main`. *Not built:* `tools/install.sh` installs it, as above.

- **Velopack packages, installs and updates the app.** One tool makes the installer and the updates for macOS, Windows and Linux, and an update downloads only what changed. Sparkle checks more for itself, but covers only the Mac, and an updater of our own would have to rebuild Velopack's Windows installer. The app reaches Velopack through one module, so replacing it would touch nothing else.
- **CI builds, signs and publishes every merge to `main`.** The merge queue has already run the checks by then.
- **The feed is signed, and the app checks the signature before trusting anything in it.** Velopack only checks each package against a checksum in the feed, so without a signature, whoever controls the host controls the app. *Unconfirmed.*
- **The hub runs from its own copy for each build, outside the app's install directory.** An update replaces that directory, and on Windows Velopack first kills everything running from it, which would stop the hub and its agents. *Unconfirmed.*
- **The app decides when an update installs,** so an update doesn't interrupt what someone is doing.

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
- **The contract is a Rust trait for now.** It becomes messages when the first external adapter needs them. `snapshot` and `restore` come with the first provider that uses them.
