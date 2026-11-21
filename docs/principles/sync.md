# Keeping a client's copy in sync

These rules keep a client's copy of a hub's state correct through lag, reconnects and restarts at either end, without each view working out how.

## What a hub serves

- **Everything a hub serves is a log, a record or live output.** Each kind has one way to tell a newer copy from an older one, and one way to recover.

  | Kind | What | Newer or older | Recovering |
  |---|---|---|---|
  | Log | A session's events, or only the agent's own, or one subagent's | By `seq`, or by place among the events of the same agent or subagent. Neither has gaps | Subscribe again from one past the last `seq` or place. Read the log again from a hub of another build |
  | Record | Repositories, workspaces, sessions, comments, hub settings, agents' accounts, GitHub accounts, sign-ins with GitHub, apps of your own, whether the default can push to each repository, per-user state, a workspace's terminals, and each session's subagents, proposals and how each reply read out ended | By revision, `rev`, from one counter per hub, which moves in the same transaction as the change. A session's newest event moves none, since the log says how far a session has got | List again, and keep the copy with the newer revision |
  | Live | Output that isn't stored: a session's tail, what a terminal shows, and what GitHub says of what a client shows | Nothing to compare. It belongs to one run of an agent, one terminal or one device's connection, and arrives in order on one connection | Replace it with a new snapshot |

- **For logs and records, a client decides which copy is newer item by item, so the order copies arrive in doesn't matter.** A client drops a duplicate or an older copy by its `seq` or revision. So it doesn't depend on how a list and a feed notice interleave. A list is read in one transaction and carries the revision it was read at, so a record missing from the list is gone, unless the client holds a newer copy from the feed. Removing a record is a change with a revision of its own.
- **A log's events don't change once recorded, except in a migration.** A migration can rewrite events stored in an older form, and a rewritten event keeps its `seq` and place. So a client that reaches a hub of another build reads the log again. A hub migrates before it replies to a hello, so no client reads a log part way through a migration.
- **A record that lasts only as long as a process, or that only a process can state, is kept in memory and not stored.** A stored copy would be out of date as soon as the process went. Its revision still comes from the hub's one counter, so clients order it among the other records. No transaction holds the change, so the record changes and takes its revision under a lock that listing the records takes too. For example, a terminal's record lasts as long as its worker, and a list of models is whatever the agent's CLI last reported. *Unconfirmed.*
- **A subscriber reads events from the log, and a live update only signals that the log advanced.** A slow client falls behind and catches up without losing anything, and it delays only itself.
- **Live output isn't stored, and a client that attaches or falls too far behind gets a new snapshot.** Nothing then queues for a slow client without limit. For example, a client that attaches in the middle of a message gets a snapshot of the tail and then the deltas, and the finished message arrives as an event and replaces them. A client on a slow link is sent a terminal's screen as it is now, rather than all that came between.
- **What a worker watches reaches the hub only from the workspace's current worker, and each report replaces the one before it whole.** A report from a worker that's been replaced is dropped, and nothing is merged. For example, a worker sends every branch of its worktree each time one moves. *Unconfirmed.*
- **Only the current run's live output reaches clients.** Each launch of an agent has a run ID, which every event and delta carries, and a later launch always has a higher one. The hub drops live output from any other run. A late event from a replaced run still goes in the log, but it can't change the session's status or clear its replacement's tail.

## IDs

- **Every ID has one allocator, and is unique only within its scope.** Nothing treats an ID as unique where it might not be.

  | ID | Allocator | Unique within |
  |---|---|---|
  | Repository, workspace, session, comment | The hub, as a UUID | Everything, so one sidebar can merge several hubs |
  | Child worktree, wait, proposal, paired device | The hub, as a UUID | Everything |
  | GitHub account | The hub, as a UUID, because one GitHub user can be connected more than once | Everything |
  | Terminal, device on the hub's machine | A client, as a UUID, so a terminal opened again opens once | Everything |
  | A terminal's row number and numbering | The worker | A terminal |
  | `seq`, run | The hub | A session |
  | Place | The hub | The agent's own events, or one subagent's, in a session |
  | Revision | The hub | A hub |
  | `client_msg_id` | A client | A session |
  | Attachment | A client, or the worker for an image a tool returned, each as a UUID | A session |
  | Ask key | The worker, as a UUID or the agent's ID for the tool call | A session |
  | Message, tool use, approval | The agent | A session. An agent may reuse one elsewhere, so it's never a key outside its session |
  | `client_request_id`, sign-in with GitHub | A client | A hub |
  | Subscription | A client | A connection |
  | Account | The agent's adapter, from the login, the same in every build. The hub, for a login it holds that doesn't say whose it is | An agent's kind. Each ID starts with the kind's name |
  | GitHub user | GitHub: a number that never changes, and a username that can. The hub tells users apart by the number | GitHub |
  | App of your own, installation | GitHub: a number each. The hub keys an app's record by its number, and an installation's account by the app and the user or organization it's on | GitHub |

- **An item keeps one identity through every state it passes through.** A prompt is its `client_msg_id`, from pending to recorded. A block of the agent's output is its message ID and index, from its first delta to the event that records it. Anything with no identity before it's recorded is its `seq` and its position in that event. The transcript builds element IDs from these in one function, so nothing on screen resets when a prompt is recorded or a block finishes.

## In a client

- **`client-core` keeps each hub's copy, and views only read it.** For each hub, `client-core` holds the records, a window of each open session and the rows near the view of each open terminal, and it does all the subscribing, fetching and recovering. It takes in the record a request's reply carries before the request returns, so no view passes a reply on. It doesn't take in a list from a reply, because only the feed's own list says which records are gone. It merges the updates a view hasn't looked at, so a slow view sees the latest state and not a backlog. It hands out an open session's log one version at a time, and a version never changes, so every row of a frame agrees with the others. A view keeps only per-device state, such as scroll position and selection, keyed by ID, so it survives a reconnect. A terminal's are in `client-core` instead, kept by row number through a reconnect, unless its rows were numbered again meanwhile, when they start at the bottom with nothing selected. It also connects again whenever the connection goes, by the way to connect each app gives it.
- **A client holds only a window of a long session's events, so anything about the whole session is a request to the hub.** The hub reads only the event kinds and fields the request needs, so the cost doesn't grow with the rest of the session. For example, `session_prompt_list` reads only prompts and the events that take or drop them.
