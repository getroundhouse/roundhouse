# The microsandbox adapter embeds microsandbox's library, and its workers dial the hub

**Status:** Decided, built in part: a workspace runs in a sandbox made from a stock image, without base images.
**First recorded:** 2026-11-20, in #2115.

The microsandbox provider runs each workspace in a microVM of its own on the hub's machine. Its adapter, `roundhouse-microsandbox`, is a program of its own that the hub runs, as every adapter is (`provider-adapters-over-stdio.md`, `providers-reach-their-own-machines.md`), and calls microsandbox's Rust library, rather than running the `msb` command line and reading what it prints. The owner chose it on #2115.

The library runs each VM in a process of its own, `msb machine`, so only `msb` is signed with the hypervisor entitlement. It comes from microsandbox's own install on the hub's machine, named with `--msb`.

The adapter only makes, starts, stops and destroys sandboxes. Each sandbox's worker dials the hub itself, with the ticket the hub gives at `start`, as `workers-dial-the-hub.md` says, so the adapter is never in a worker's stream:
- **A UDP port on the hub's machine is forwarded into each sandbox,** to the port the worker binds there. The worker advertises `127.0.0.1:<port>`, so the hub opens a direct path to it, since UDP from a VM out to its host never forms one, as #2451 found.
- **The worker starts itself in a session of its own,** with `roundhoused worker --ticket … --detach`, and keeps its own log, since a process run in a sandbox ends with the command that ran it.
- **The worker ends itself and what it left,** with `roundhoused end-worker`, which finds them as the local provider does, so the adapter holds no script for it.
- **Before it starts the worker, the adapter checks it's the hub's build,** by its `--version`, and fails the start saying so if it isn't, since `providers.json` names the builds by hand until #2032 and #600 tie them to the hub's. *Unconfirmed.*

How a sandbox gets what a workspace needs, until #2198 settles it for every provider:
- **A stock image,** `ubuntu:24.04`, set up once it has booted by a script the adapter carries, within ten minutes. It installs `git` and `curl`, makes the user the worker runs as, and installs the agents' command lines. Neither the image nor the script can be changed with a flag, since the script, the user and the paths the adapter runs the worker at all assume them. *Unconfirmed.*
- **The worker** is the one the hub gives at `start`, as the provider protocol has it, written into the sandbox and kept while its checksum matches.
- **No credentials** go into a sandbox, since the hub hands them to the worker at each launch.

## Chosen over

- **Driving the `msb` command line,** which was known to work from a launch daemon (`docs/reference/launchd.md`). The library gives typed calls and errors.
- **Relaying the worker's stream through the library's `exec`,** as the adapter did before workers dialed, which put it in every byte the worker sent.

## What it costs

- **About a hundred crates** the rest of the workspace doesn't use. The adapter is left out of the default members, so `cargo test` doesn't build them.
- **`msb` has to be installed on the hub's machine,** at the library's version. Shipping it with the adapter would need our release to sign it.
- **A sandbox needs the internet to connect to its hub,** since a worker's connection starts through n0's relay, until a hub can name a relay of its own. Its data then goes over the forwarded port.
