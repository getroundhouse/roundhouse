# Records

These rules cover records: what one is, how one changes, and how a client keeps its copy. `docs/principles/serving.md` says what else a hub serves, and `docs/principles/state.md` who owns a piece of state.

## What a record is

- **A record is state the hub owns: what Roundhouse decides, what people enter, and what the hub recorded of a moment that's past.** What the hub only observes is an observation, as `docs/principles/observations.md` says, even where the hub decides from it. For example, a workspace's branches are git's, so they're an observation, and which pull request the hub worked out from them is a record. The branch an archived workspace had checked out is a record too, since the hub recorded it as the worktree went.

  ```rust
  // crates/protocol/src/model/record.rs, abridged
  /// One record of any kind, as a list or the feed gives it.
  pub enum Record {
      Repo(Repo),
      Workspace(Workspace),
      Session(Session),
      Comment(DiffComment),
  }

  // crates/protocol/src/model/workspace.rs, abridged
  pub struct Workspace {
      pub id: WorkspaceId,
      pub rev: Rev,
  }

  // crates/protocol/src/model/counters.rs, abridged
  /// When a record last changed, from one counter per hub, so a client can tell which of two copies is newer: a copy
  /// at a higher one replaces one at a lower.
  pub struct Rev(u64);

  // crates/protocol/src/client/feed/notices.rs, abridged
  /// A record changed. Carries the whole record and its revision, so a client only ever replaces its
  /// copy with a newer one.
  pub enum HubNotice {
      Workspace(Workspace),
      Session(Session),
      /// The record is gone, as of `rev`.
      Removed {
          record: RecordKey,
          rev: Rev,
      },
  }
  ```
- **These are records:** repositories, workspaces, sessions, comments, hub settings, agents' accounts, GitHub accounts, sign-ins with GitHub, apps of your own, tool servers, per-user state, a workspace's terminals, and each session's subagents, proposals and how each reply read out ended.
- **A record tells "not known" apart from "none", and nothing records a guess.** For example, a subagent's record has no count of the tokens its conversation holds until its agent has said one, and a client then says nothing of its context, where 0 would say it's empty. An adapter that can't tell why its agent started a turn by itself records no `OwnTurn` event.
- **State that no client shows stays off the records that clients sync.** The hub stores it in a table or a row of its own, and changing it moves no revision. For example, the hub stores a workspace's `RepoSource` and `WorkerEnvironment` in SQLite apart from the workspace's record, and the port a worker serves its agents' tools on is in a table of its own. A tool server's record says the header its key goes in, or whether its sign-in works, and the key or the tokens are in a file, and what opens the server is in what workers are sent.
- **A record that lasts only as long as a process, or that only a process can state, is kept in memory and not stored.** A stored copy would be out of date as soon as the process went. Its revision still comes from the hub's one counter, so clients order it among the other records. No transaction holds the change, so the record changes and takes its revision under a lock that listing the records takes too. For example, a terminal's record lasts as long as its worker, and whether the hub is asking an agent which models it offers, and how that went, lasts as long as the hub, while what the agent offers is an observation, as `docs/principles/observations.md` says. *Unconfirmed.*

## Changing one

- **A record that repeats what the log holds changes in the same transaction as the event that changes it.** A client that reads the record after receiving the event then can't see an older value. A session's status and its open approvals both change this way, and a record's revision moves in the same transaction as the change.
- **The hub checks a request that's only safe in one state as it acts on it, and a client never acts on what it last read.** A client's copy can be a moment old, so something can start between its read and its request. The request says what it needs, and the hub refuses with a code the client can tell apart. For example, `hub_shutdown_if_idle` stops the hub only if it's still idle, and otherwise returns `Conflict`, so a prompt sent after the app last saw the hub idle isn't cut off. *Unconfirmed.*

## Keeping a copy

- **A change sends the whole record with its revision, and a client keeps whichever copy has the newer one.** The revision, `rev`, comes from one counter per hub, which moves in the same transaction as the change. A session's newest event moves none, since the log says how far a session has got. To recover, a client lists the records again, and keeps the copy with the newer revision, as `docs/principles/sync.md` says. The first list of a level on a connection replaces what the client holds of that level, whatever the revisions, so a hub whose revisions started again, as after its database is replaced from a copy, is shown at once.
