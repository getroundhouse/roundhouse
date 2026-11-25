# Who owns each piece of state

These rules say who owns a piece of state, what an app keeps on its device, and how what the hub stores changes form. Decide who owns the state before choosing where a change goes. `docs/principles/serving.md` says which kind of thing it then is, and each kind has a doc of its own.

## Owners

- **Each piece of state has one owner, and every other copy is a cache.** For example, git and GitHub own code, pull requests and CI, and the worker and the hub only cache them.
- **State that every client shows the same is the hub's, and the hub records it instead of computing it or fetching it live.** The test: should a second client, or this client after a restart, show the same thing? If so, the hub owns it and a client only draws it. For example, a GitHub sign-in in progress is a record of the hub's, so every client shows the same code, and a client that reconnects to the same hub picks the sign-in up again.
- **A client keeps or computes only what's about its own device.** For example, a client owns which workspace and chat are open, and the scroll position. A fix that finds a client keeping or computing more moves the state to the hub, usually with a protocol change, even when fixing the client would be the smaller change.
- **State that's one person's is kept for that person, on the hub.** How far a chat is read, and what's typed and not sent, belong to whoever read or typed, not to the chat. *Not built:* a hub has one user, so these are on the session's and the workspace's records. *Unconfirmed.*
- **A client reads a fact from the record or event that states it, and never infers it from the events it has loaded.** A client holds a session's log in pages, so the event that settles a question can be in a page it hasn't loaded. For example, a session's record lists the approvals the agent is waiting on, so a client doesn't search its loaded events for a request with no answer.
- **State is keyed on the thing it describes.** Whether the default GitHub account can push to a repository is keyed on the repository, not on its project or a workspace, so it still holds when a project has more than one repository. A pull request belongs to its branch on GitHub, so two workspaces on the same branch have the same pull request.

## What an app keeps on its device

- **Each kind of thing an app keeps on its device has a type of its own, named for what it is, and what uses it never sees how it's stored.** Each kind needs something different, so one store for files, or for everything, would fit none of them. *Unconfirmed:* what each needs.

  | Kind | What it is | What it needs |
  |---|---|---|
  | Settings | What a person chooses in Settings | Shows as it changes, and says when it can't be saved |
  | Preferences | What the app remembers from how it's used, such as the workspace it was left on | Saved quietly, since they change often and losing one costs little |
  | Paired hubs | The hubs this device paired with | Never lost, and forgetting one forgets its secret |
  | Secrets | The token a paired hub gave this device | Encrypted, and for this device only |
  | Recordings | What someone dictated, and what was around it as they did, once they chose to keep it | Only its user can read it, written as it's said, so the app going loses none of it, and never sent |
  | Downloads | Such as the model dictation hears with | Left out of backups, since they can be downloaded again |

- **A kind has a trait only where platforms store it differently, which is only secrets.** Any other is one type that every app with that kind uses, given where to keep it, since a trait would have one implementation. *Not built:* that trait, and one type for paired hubs. The desktop app keeps its paired hubs in its settings, with a few preferences, such as which projects are collapsed, and the iPhone app keeps its one hub its own way. *Unconfirmed.*

## What's stored

- **What a newer build stored in a form this build can't read is kept as it is, and never taken for nothing and deleted.** Going back a build then loses nothing. For example, a login file in a newer form stays in `github-logins/`, and its account blocks until a build that can read it runs. *Unconfirmed.*

## The database

These rules hold for the hub's database, not for its cache of what it reads from GitHub, which is a file of its own that the hub empties rather than migrates, as `docs/principles/keeping-observations.md` says.

- **The database migrates forward only.** Migrations run in order, each in its own transaction. A hub rejects a database from a newer build, so it can't damage it.
- **A hub refuses a database older than its floor, the oldest release it updates from, and leaves it untouched,** so that release can still migrate it. For example, a hub whose floor is 0.1.300 exits on a database 0.1.250 left, and its log says to run 0.1.300 first. Only a build installed by hand, or one taken after the hub failed to start, meets this, since updates step through floors. The floor moves only at a break in the worker protocol. *Unconfirmed:* that only the hub's log says so.
- **The hub copies the database before it migrates it, and doesn't start if the copy fails.** Any update can bring a migration that nobody chose, and the previous build can't open the migrated database. Because of the copy, the hub stores attachment bytes as files in its data directory, and not as rows.
- **When what's stored changes form, the hub rewrites it once, in a migration, and converts what a worker of the previous build still sends as it arrives.** No client then reads an older form, and only that migration and the hub's reading of what such a worker sends know it. They read it and never write it. For example, a migration typed each tool call that Claude's adapter had recorded by its tool's name. *Unconfirmed.*
