# microsandbox

How microsandbox behaves under the adapter, as found on a Mac Studio with Apple silicon on macOS 27.2.1, with microsandbox 0.7.16, run as a launch daemon as a user who isn't an administrator. `docs/decisions/microsandbox-adapter.md` says why the adapter embeds its library.

- **The library runs each VM in a process of its own,** `msb machine`, which it finds from `MSB_PATH`, a path it's given in code, its configuration, or its home, `~/.microsandbox`, with `libkrunfw` beside it. Homebrew's `msb` is signed with `com.apple.security.hypervisor` and `com.apple.security.cs.disable-library-validation`, and the program calling the library needs neither.
- **A sandbox outlives the program that made it.** `msb machine` leads a process group of its own, and a later program reconnects to the sandbox by name.
- **A process run in a sandbox ends with the session that ran it,** unless it starts itself in a session of its own, as `roundhoused worker --attach` starts the worker it connects to.
- **A session stays open while anything holds its stdout or stderr,** after the process it ran has exited. So the worker `--attach` starts, which keeps the stderr it was given, kept `connect` running after the hub closed its stream, until its stderr went to a file instead.
- **Booting `ubuntu:24.04` takes about two seconds,** and setting it up with the adapter's script about twenty. Writing a 64 MiB worker into a running sandbox took a tenth of a second.
- **A stock `ubuntu:24.04` has `bash`, `ps` and glibc 2.39, but no `git`, `curl` or CA certificates.** The Linux worker from a release, built on Ubuntu 22.04, runs in it, needing only `libc`, `libm` and `libgcc_s`.
- **Streaming a process's input and output** carried 64 MiB through `cat` intact at 135 MB/s, and a line in 0.15 ms.
- **A running sandbox's disk snapshots in a tenth of a second, and restores as a new sandbox in a third of one.** A restore names the snapshot as `<sandbox>:<snapshot>`, and a name an earlier snapshot of the same sandbox used conflicts until that one is removed.

## Setting the adapter up on a hub's machine

Until the hub's settings name providers, its `providers.json` does, as `crates/hub/src/provider_set/mod.rs` says. With microsandbox installed by Homebrew, and the Linux worker from the release the hub runs, for the hub's processor:

```json
{
  "default": "microsandbox",
  "providers": [{
    "name": "microsandbox",
    "program": "/path/to/roundhouse-microsandbox",
    "args": ["--msb", "/opt/homebrew/opt/microsandbox/libexec/msb", "serve"]
  }],
  "workers": [{"target": "aarch64-unknown-linux-gnu", "path": "/path/to/linux/roundhoused"}]
}
```

`serve --image` names another image to make sandboxes from, and `serve --setup` another script to set each up with.
