# The microsandbox adapter embeds microsandbox's library

**Status:** Decided, built in part: a workspace runs in a sandbox made from a stock image, without base images.
**First recorded:** 2026-11-20, in #2115.

The microsandbox provider runs each workspace in a microVM of its own on the hub's machine. Its adapter, `roundhouse-microsandbox`, is a program of its own, as every adapter is (`provider-adapters-over-stdio.md`), and calls microsandbox's Rust library, rather than running the `msb` command line and reading what it prints. The owner chose it on #2115.

The library still runs each VM in a process of its own, `msb machine`, so `msb` is what's signed with the hypervisor entitlement, and the adapter, the hub and the app need none. `msb` and its `libkrunfw` come from microsandbox's own install on the hub's machine, which the adapter is pointed at with `--msb`.

How a sandbox gets what a workspace needs, until #2198 settles it for every provider:
- **A stock image,** `ubuntu:24.04` unless the adapter is told another, set up once it has booted by a script the adapter carries. It installs `git`, `curl` and CA certificates, makes the user the worker and its agents run as, and installs the agents' command lines.
- **The worker** is the one the hub gives at `start`, as the provider protocol has it, written into the sandbox and kept while its checksum matches.
- **No credentials** go into a sandbox, since the hub hands them to the worker at each launch.

## Chosen over

- **Driving the `msb` command line,** which was already known to work from a launch daemon (`docs/reference/launchd.md`). The library gives typed calls and errors, and its `exec` streams a process's input and output, which the adapter's `connect` needs, without parsing what a command prints.

## What it costs

- **About a hundred crates** the rest of the workspace doesn't use, among them a SQLite layer. The adapter is left out of the workspace's default members, so `cargo test` doesn't build them, and `--workspace` does.
- **`msb` has to be installed on the hub's machine,** at the version of the library. Shipping it with the adapter, which the library can carry inside it, isn't tried, and would need it signed with the entitlement by our release.
