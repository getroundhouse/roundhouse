# The parts and what each one does

These rules say which process does which job: hubs, workers, providers and clients. `docs/reference/terms.md` defines each part.

## Hubs

- **A hub owns the state, and a worker does the work.** The hub stores workspaces, sessions and their event logs, and serves the API. A worker runs a workspace's agents, terminals and git, and reports events to its hub. For example, when a workspace is created, the hub records it and tells a worker to create the worktree and start the agent. A workspace belongs to one hub.
- **Every hub runs the same code and serves the same API, wherever it runs.** A local hub, a self-hosted hub and a hosted hub differ only in storage and authentication. Separate local and cloud code would duplicate every feature. *Not built:* the self-hosted and hosted hubs.
- **The hub is a process of its own that outlives every client, so whatever must happen with no client connected is the hub's job.** Closing the app stops no agent. For example, the hub sends a queued prompt when a turn ends. When the hub's settings allow a tool use that an agent asks about, the hub approves it itself, because the agent waits on the hub whether or not a client is open.
- **The hub doesn't know what starts, updates or stops it.** Who does that depends on where the hub runs: a client, for a local hub that was started on demand, and a person, systemd or Docker, for a hub someone runs themselves. The hub only reports its status and shuts down cleanly on `SIGTERM`. It has no desktop mode and doesn't depend on the app.
- **The hub replies from what it holds without starting a worker, wherever that can say.** Then a workspace nobody is using doesn't keep a worker running. For example, the hub holds a workspace's branches and where each is pushed, as its worker last sent them, so the hub works out which pull request is the workspace's with no worker.
- **What the hub does by itself never undoes what a person chose.** For example, the hub's checks leave an installation the person disconnected as disconnected, though GitHub still lists it, and only the person changes the default GitHub account once there's one. *Unconfirmed.*

## Workers and providers

- **A worker gets what only its machine has, and the hub gets what comes from people and from GitHub.** A worker reads what git reports about its checkouts and worktrees, their files, its processes and the machine's own `gh` login. The hub runs no git. It calls GitHub itself, so GitHub's data is available with no worker running, and every client and agent reads the same data. The hub reads the `gh` login on its own machine, and on no other, once a person connects it as a GitHub account, since an account's token has to be there with no worker running.
- **Each workspace has its own worker, in a process of its own and in its own provider instance.** A crash in one workspace doesn't affect another, and sandbox providers already give each workspace its own machine. The worker is a separate process even on the hub's machine, so a remote worker changes only the transport.
- **No provider is special, and any hub can use any provider.** The hub reaches the local provider as it reaches any other: over the provider protocol, to an adapter in a process of its own. Then every workspace uses the protocol, and a new provider is only a new adapter. *Not built:* the hub runs the local provider inside itself, through the `Provider` trait.
- **A worker needs only a byte stream to its hub, and the hub opens it.** A provider's `connect` returns a command. The hub runs the command and uses its stdin and stdout as the worker's stream, so the transport can be `incus exec`, SSH or the adapter's own program. The hub dials out, so a worker needs no route to the hub. No adapter sits in the stream, so an adapter can stop without stopping its workers.
- **A provider adapter works only with its own hub's instances.** The hub makes an ID once, keeps it, and gives it in its hello to each adapter, so several hubs can share a host. *Not built:* the hub listing an adapter's instances as it starts, and reporting any that none of its records names rather than destroying it, since a database restored from a backup has no record of an instance made since, which can hold live work.
- **A provider declares its capabilities and its workers' environment, and the hub assumes neither.** For example, a provider declares whether its workers share the hub's filesystem. An adapter that doesn't declare it gets `WorkerFilesystem::Separate`, so the hub never sends it a path on the hub's disk.

## Clients

- **Clients talk only to hubs.** A client never touches git, a terminal or an agent directly. For example, the desktop app requests a diff from the hub, and the hub fetches it from the worker.
- **A hub treats whoever can reach its local socket as its user, and a client on the network can't manage the hub.** `hub_status`, `hub_shutdown` and `hub_restart` work on the local socket without a hello. A client on the network authenticates before its hello, and can't stop or restart the hub or manage paired devices.
