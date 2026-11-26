# The hub has tool servers of its own and holds their sign-ins, and agents reach them through the worker

**Status:** Decided. Partly built: not sign-ins.
**First recorded:** 2026-11-24, in #2675.

A person adds a tool server, which is an MCP server over HTTP, to the hub once and signs in to it once. Every agent in every workspace then has it, whichever CLI it runs and machine it's on.

A server is a name, a URL and how it knows the person: a sign-in, a key in a header, or neither. Each server has at most one sign-in, which goes when the server is removed. So someone with two accounts at one URL adds two servers. Every server is in every project. The hub is the OAuth client, and only the hub refreshes a token.

Each agent is passed the URL of a proxy on its worker, at 127.0.0.1, and not the server's own. The worker forwards the MCP traffic as it is, with an access token or key the hub hands it. When the server refuses the token, the worker asks the hub for a new one and tries again. If that fails, the agent gets an error it can read, not the 401.

Servers are set up and signed in to in the desktop app, which opens the browser, catches the redirect and gives the hub the code. The iPhone app has none.

The hub's servers are apart from those in a CLI's own settings, which Roundhouse neither copies nor changes. Each CLI does what it does with the hub's.

## Chosen over

- **Each CLI's own sign-in.** A person signs in for each CLI on each machine, and an instance that's destroyed loses it. Copying one sign-in to several doesn't work: MCP has authorization servers rotate a public client's refresh tokens, so one copy's refresh can end the others.
- **One sign-in for each URL.** A person couldn't have their own account and a client's at one server.
- **A server that lists several accounts.** It needs records for the accounts, and a choice of one for each project.
- **A choice of which projects have a server.** It can be added later.
- **A token passed to the agent at launch.** It ends while the agent is running.
- **The hub making each call to the server.** A token would never leave the hub, but an agent could reach only what the hub's network can.
- **One order of precedence between the hub's servers and a CLI's own.** The CLIs' orders differ, so Roundhouse would have to override each's settings.

## What it costs

- **An access token, or a key, is on the worker's machine,** where an agent with full access might reach it. The refresh token isn't.
- **A server whose name is also in a CLI's own settings behaves differently in each CLI,** as `docs/reference/` says of each. Claude uses the hub's, Antigravity its own, and Codex merges the two. Codex starts no chat where its own is over stdio.
- **An agent already running may not get a server signed in to after it launched,** since its CLI had no tools to list.
- **An agent in any project can read and change what any server reaches,** such as a client's Linear from a personal project.
- **Servers over stdio stay with the CLI's settings and the repository.**
- **The site keeps Roundhouse's client document at `https://getroundhouse.dev/oauth/client.json`,** which can't move while any sign-in made with it lasts. MCP prefers one to registering with each server, now deprecated.
