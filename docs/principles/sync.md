# Keeping a client's copy in sync

These rules keep a client's copy of a hub's state correct through lag, reconnects and restarts at either end, without each view working out how. `docs/principles/serving.md` says what a hub serves, and each kind's own doc how a client keeps its copy of it.

## Every kind

- **For events, records and observations, a client decides which copy is newer item by item, so the order copies arrive in doesn't matter.** A client drops a duplicate or an older copy by its `seq` or revision. So it doesn't depend on how a list and a feed notice interleave. A list is read in one transaction and carries the revision it was read at, so a record missing from the list is gone, unless the client holds a newer copy from the feed. Removing a record is a change with a revision of its own.

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
