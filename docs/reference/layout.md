# Layout

A map for finding your way before reading code: the parts and what connects them, what each crate is, and who owns each piece of state. `docs/principles/parts.md` has the rules the parts follow, and `docs/principles/layers.md` which crate can depend on which.

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
  transcript/      A chat's history and live output, in GPUI
  terminal-state/  What a terminal shows, on Ghostty's terminal core
  terminal-view/   A terminal, in GPUI
  look/            The design system: colours, type, sizes, components and icons
  dictation/       Speech to text on the device
  dictation-ui/    Dictation in a text field, in GPUI
  now-playing/     Pausing what's playing while dictating, on a Mac from inside perl
  fetching/        Downloading files over HTTP
  update-feed/     What a release adds to the update feed
  windows-icon/    An icon as the resource a Windows program carries, for the desktop app's build
  waiting/         How tests wait
apps/
  daemon/          `roundhoused`, which runs as a hub or a worker
  cli/             `roundhouse`, the command-line client
  desktop/         `roundhouse-desktop`, the desktop app
  mobile-ios/      `roundhouse-ios`, the iPhone app
  microsandbox-adapter/  `roundhouse-microsandbox`, the adapter of the provider that runs each workspace in a microVM
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
