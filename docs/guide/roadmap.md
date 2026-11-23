# Roadmap

What's built, and what's designed and not built. The order things are built in, and how each is going, is on GitHub: [milestones](https://github.com/getroundhouse/roundhouse/milestones) for the order, and [issues](https://github.com/getroundhouse/roundhouse/issues) for each piece of work.

## Where it stands

| | |
|---|---|
| **Mac** | Built, for Apple silicon |
| **Windows** | A first preview, for amd64 and ARM64. Unsigned |
| **Linux** | A first preview, as an AppImage for amd64 and ARM64 |
| **Agents** | Claude, Codex and Antigravity, through their CLIs; Codex and Antigravity are exercised on macOS |
| **The hub** | Runs on your machine; another paired desktop can connect over a LAN or VPN |
| **iPhone** | Built from source with Xcode, and not on the App Store; it pairs with a hub on the same network or a VPN, lists its workspaces and works in their chats |

[Install](get-started/install.md) says what each preview needs, and how it differs from the Mac.

## Designed, and not built

These are part of the design, and the app is built to take them. None is built.

- **More agents,** beyond Claude, Codex and Antigravity.
- **Codex and Antigravity logins managed by the hub,** rather than their worker machine’s native sign-ins.
- **A hub on your own server, or in the cloud,** with workspaces in VMs or sandboxes rather than on your machine.

## Smaller things that aren't built

- **Search in the sidebar.** The button is there, and does nothing.
- **A workspace from another branch or a pull request,** and linking one to an issue.
- **A page of what each update changes.**
- **A sound on Windows and Linux,** when an agent needs you.
- **Dictating a prompt on Windows and Linux.**
- **Updates on Windows that don't wait for agents.**
- **Alt opening the menus on Windows and Linux,** as `F10` does.
- **Selecting all of what an opened call shows,** such as a file's lines, a diff, and output over 16 KB.

## Say what's missing

If something you need isn't here, [open an issue](https://github.com/getroundhouse/roundhouse/issues). What the product does is settled there first.
