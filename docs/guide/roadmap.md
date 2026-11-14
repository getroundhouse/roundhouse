# Roadmap

What's built, and what's designed and not built. The order things are built in, and how each is going, is on GitHub: [milestones](https://github.com/getroundhouse/roundhouse/milestones) for the order, and [issues](https://github.com/getroundhouse/roundhouse/issues) for each piece of work.

## Where it stands

| | |
|---|---|
| **Mac** | Built, for Apple silicon |
| **Windows** | A first preview, for amd64 and ARM64. Unsigned |
| **Linux** | A first preview, as an AppImage for amd64 and ARM64 |
| **Agents** | Claude and Codex, through their CLIs; Codex is exercised on macOS |
| **The hub** | Runs on your machine |

[Install](get-started/install.md) says what each preview needs, and how it differs from the Mac.

## Designed, and not built

These are part of the design, and the app is built to take them. None is built.

- **More agents,** beyond Claude and Codex.
- **Codex logins managed by the hub,** rather than its worker machine’s native sign-in.
- **A hub on your own server, or in the cloud,** with workspaces in VMs or sandboxes rather than on your machine.
- **An iPhone app,** as another way in to the same hub.

## Smaller things that aren't built

- **Search in the sidebar.** The button is there, and does nothing.
- **Lines added and removed, on each workspace's row.**
- **A workspace from another branch or a pull request,** and linking one to an issue.
- **A page of what each update changes.**
- **A sound on Windows and Linux,** when an agent needs you.
- **Updates on Windows that don't wait for agents.**
- **Alt opening the menus on Windows and Linux,** as `F10` does.
- **Selecting all of what an opened call shows,** such as a file's lines, a diff, and output over 16 KB.

## Say what's missing

If something you need isn't here, [open an issue](https://github.com/getroundhouse/roundhouse/issues). What the product does is settled there first.
