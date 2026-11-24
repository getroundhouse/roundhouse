# What a hub serves, and how a client gets it

These rules say what a hub serves, how a client gets it, and what the hub keeps current. Each kind of thing a hub serves has a doc of its own, which the table names. `docs/principles/state.md` says who owns a piece of state, and `docs/principles/sync.md` what's common to every kind in a client.

## The four kinds

- **Everything a hub serves is an event, a record, an observation or a live snapshot.** Each kind has one way to tell a newer copy from an older one, and one way to recover. What fits none of them is a question for the owner before anything is built on it.

  | Kind | What it is | Where the hub keeps it | How a client keeps its copy | Its rules |
  |---|---|---|---|---|
  | Event | Something that happened in a session, such as a prompt, a tool call or a turn ending. It never changes once recorded | The database | Appends it, in order, by `seq` | `docs/principles/events.md` |
  | Record | Current state the hub owns, such as a workspace, a session's status or a comment | The database, or memory for one that lasts only as long as a process | Replaces it with a newer copy, by revision | `docs/principles/records.md` |
  | Observation | The hub's copy of state someone else owns, as the hub last observed it, such as a workspace's lines added and removed, what GitHub says of a pull request, or how full an agent's context is. *Not built:* in the hub, any but a workspace's lines added and removed, its changed files, what GitHub says of a chip's issue or pull request and of a workspace's own, whether the default GitHub account can push to a repository, what GitHub calls a GitHub account's user, what an agent offers where a chat would launch, who an agent's account is, an account's usage limits, how full an agent's context is and its to-do list, what the API's cache holds of a conversation, and what each open wait last read | Memory, and a cache file | As a record | `docs/principles/observations.md` |
  | Live snapshot | What something running shows now: a reply as its agent writes it, or a terminal's screen | Not stored | Takes it whole, then each change | `docs/principles/live-snapshots.md` |

## How a client gets it

- **A client gets what a hub serves in two ways: it subscribes, or it sends a request and gets a reply.** A subscription is for what it goes on showing. A request is for what it reads once, such as an older page of a chat or one file's diff, and for changing something, such as sending a prompt.
- **There's one way to subscribe: a client says what it follows, each at one of a few levels, and for each the hub sends where it stands and then every change.** After a reconnect the client says again what it follows, and how far it had got in each log. A client keeps what arrives by its kind, as the table above says, whichever level sent it, so an observation has a revision at every level.

  | A client follows | And is sent |
  |---|---|
  | The hub | Every record but those of one session, and each observation that's drawn for every workspace at once, as a sidebar draws each workspace's lines added and removed and how its pull request stands |
  | A workspace | The observations only its own pane draws, such as its changed files and its pull request's checks and reviews |
  | A session | Its events, from the `seq` or place the client names, and the live snapshot of the reply being written. The records of that session alone: its subagents, its proposals and how each reply read out ended. And the observations only its own pane draws, such as how full its agent's context is |
  | A terminal | The live snapshot of its screen |
  | An issue or a pull request | The observation of it that a chip draws |

  *Not built:* the observations sent are a workspace's lines added and removed, at the hub's level, its changed files, at its own, and what GitHub says of a workspace's own pull request, at the hub's level and the workspace's, and of an issue or a pull request, at its own, whether the default GitHub account can push to a repository, what GitHub calls a GitHub account's user, what an agent offers, who an agent's account is, an account's usage limits and what the API's cache holds of a conversation, at the hub's level, and how full an agent's context is and its to-do list, and what each of its open waits last read, at its session's level. The hub keeps a request of its own for each of the hub's records, a session, a terminal and what a client shows of GitHub, `FeedSubscribe`, `SessionSubscribe`, `TerminalWatch` and `GithubSubscribe`, for clients of version 3.
- **The levels are a short fixed list, and a client can't narrow one by a condition.** So the hub evaluates no query to decide what to send, and every client that follows the same thing is sent the same. A new level is the owner's to add.

## What the hub keeps current

- **What a client follows is the hub's signal to keep it current, for every client.** Keeping an observation or a live snapshot current costs work: a worker watches a worktree or sends a terminal's screen, or the hub asks GitHub again. So the hub does that work for what at least one client follows, and stops when the last one stops following or its connection ends. For example, a worker sends a terminal's screen only while a client watches the terminal. And while the desktop app follows a workspace, a phone that follows only the hub is sent that workspace's lines added and removed as they change. So no client asks again on a timer. A worker watches its worktree only once something else has started it, such as a prompt or a diff asked for, since the hub starts none so that it can watch, as the owner decided, so until then a followed workspace's changes are as of their last read.
- **What no client follows is read again when something says it may have changed, and is otherwise as of its last read.** For example, a workspace's lines added and removed are read when a turn ends in it, and a pull request nobody shows is read less often the longer it comes back the same.
