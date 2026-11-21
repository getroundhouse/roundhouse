# Decisions

A decision record holds one large, one-off choice among real alternatives, such as a framework or a wire format: what was chosen, what it was chosen over, and why. A record is written once, with its date, and only its status changes afterwards: when the choice is built, or when a new record replaces it, which the old one then names.

| Record | The choice |
|---|---|
| [hub-and-workers.md](hub-and-workers.md) | A hub owns the state and workers do the work, with the same code for local and remote |
| [no-sync-engine.md](no-sync-engine.md) | Clients stay in sync through the hub's own protocol, not a sync engine |
| [session-log-not-crdt.md](session-log-not-crdt.md) | A session's history is a log that clients read by range, not a CRDT |
| [newline-delimited-json.md](newline-delimited-json.md) | The wire format is newline-delimited JSON |
| [hub-speaks-previous-version.md](hub-speaks-previous-version.md) | Only the hub speaks the previous protocol version |
| [typed-capabilities.md](typed-capabilities.md) | What an agent can do is one typed set of capabilities, which each adapter fits its agent to |
| [worker-tools-over-mcp-http.md](worker-tools-over-mcp-http.md) | The worker serves Roundhouse's tools to its agents over MCP, as HTTP on 127.0.0.1 |
| [hub-holds-agent-logins.md](hub-holds-agent-logins.md) | The hub holds each agent account's login and hands it to every launch |
| [git-credential-set-by-hub.md](git-credential-set-by-hub.md) | The hub sets the GitHub credential for every process a worker starts |
| [machine-gh-token-handed-to-hub.md](machine-gh-token-handed-to-hub.md) | While each machine's own `gh` login is the default, the hub reads GitHub with the token a worker hands it. Replaced |
| [github-identity-held-by-hub.md](github-identity-held-by-hub.md) | Agents' GitHub identity always comes from an account the hub holds, and a `gh` login on the hub's machine is one. Replaced |
| [github-on-provisioned-machines.md](github-on-provisioned-machines.md) | Roundhouse sets up GitHub only on machines it provides, and the hub has an identity of its own. Not built |
| [private-app-of-your-own.md](private-app-of-your-own.md) | An app of your own is a private GitHub App, made under each account or organization it's for |
| [github-batched-graphql.md](github-batched-graphql.md) | The hub reads pull requests from GitHub in batched GraphQL |
| [pull-request-from-pushed-branch.md](pull-request-from-pushed-branch.md) | A workspace's pull request is found from the branch it's pushed to, with no stored link |
| [provider-adapters-over-stdio.md](provider-adapters-over-stdio.md) | A provider's adapter is a separate program that the hub runs over stdio |
| [remote-workspace-vm.md](remote-workspace-vm.md) | A remote workspace runs in a VM of its own, not a container. Not built |
| [base-image-is-an-instance.md](base-image-is-an-instance.md) | A base image is an instance, built through a worker. Not built |
| [velopack.md](velopack.md) | Velopack packages, installs and updates the app |
| [linux-appimage.md](linux-appimage.md) | The Linux app is an AppImage |
| [publish-every-merge.md](publish-every-merge.md) | Every merge to `main` is published, as version `0.1.<commits on main>` |
| [hub-runs-from-its-own-copy.md](hub-runs-from-its-own-copy.md) | A hub that a client starts runs from a copy of the daemon outside the app's install directory |
| [hub-package.md](hub-package.md) | A hub on a machine of its own is installed from an archive as a service, and a client updates it. Partly built |
| [no-strong-copyleft.md](no-strong-copyleft.md) | Nothing that ships is under GPL, AGPL or LGPL |
| [gpui-not-web-view.md](gpui-not-web-view.md) | The desktop app is native Rust on GPUI, not a web view |
| [gpui-kit.md](gpui-kit.md) | The apps are built on gpui-kit, Longbridge's component kit for GPUI |
| [gpui-kit-copy.md](gpui-kit-copy.md) | gpui-kit and three parts of GPUI are built from copies in the repo |
| [zed-ios-backend.md](zed-ios-backend.md) | The iPhone app is GPUI throughout, on Zed's iOS backend |
| [transcript-on-gpui-kit.md](transcript-on-gpui-kit.md) | The transcript is drawn with GPUI and gpui-kit, not by a renderer of its own |
| [transcript-list-of-its-own.md](transcript-list-of-its-own.md) | The transcript's rows are in a list of its own, not GPUI's |
| [transcript-keeps-its-texts.md](transcript-keeps-its-texts.md) | The transcript draws its text from the kit's Markdown blocks and keeps the selection itself. *Unconfirmed* |
| [talk-with-the-chats-own-agent.md](talk-with-the-chats-own-agent.md) | Conversation mode talks with the chat's own agent, and a small model only says what it's doing while it can't answer. Not built |
| [phonon-speaks-replies.md](phonon-speaks-replies.md) | Replies are spoken by Gradium's Phonon, on the device. Not built |
| [interaction-mode.md](interaction-mode.md) | How the person talks with a chat's agent is its interaction mode, which every event is recorded under |
| [spoken-prompt-carries-instruction.md](spoken-prompt-carries-instruction.md) | A spoken prompt reaches the agent with how to talk, which its adapter adds. Not built |
| [pause-media-through-perl.md](pause-media-through-perl.md) | The app pauses what's playing with a library of its own, which `/usr/bin/perl` loads |
| [worker-holds-the-only-terminal.md](worker-holds-the-only-terminal.md) | The worker holds the only copy of a terminal, and clients show the rows it sends |

## Chosen, with no reason recorded

Each of these is a choice that was written down without an alternative or a reason. They're listed so they aren't lost. When someone records the reason, the choice gets a record of its own and leaves this list.

- SQLite stores a local hub's data. A hosted hub would use Postgres, and so would a self-hosted one if it grows.
- The code is one Cargo workspace in one repo.
- A client reaches a paired hub directly over TLS 1.3, and checks the hub's public key against the fingerprint in the pairing link. No relay and no NAT traversal is built, so the two machines must be able to reach each other, for example over a LAN or a tailnet.
- NVIDIA's nemotron model, on Core ML, turns speech into text for dictation.
- Catppuccin's icons are the desktop app's file icons.
- Signing in with GitHub uses Roundhouse's OAuth App and GitHub's device flow.
