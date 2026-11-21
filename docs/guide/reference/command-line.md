# The command line

The `roundhouse` command talks to the same hub as the app, so a workspace made in one shows in the other.

## Find it

It's inside the app:

```sh
alias roundhouse=/Applications/Roundhouse.app/Contents/MacOS/roundhouse
roundhouse --help
```

The hub starts by itself when a command needs it.

## A task from a terminal

```sh
roundhouse repo add ~/code/my-project
roundhouse workspace create my-project      # Prints the new workspace's name, such as euston
roundhouse session new euston
roundhouse send euston "Fix the login redirect"
```

`send` follows the chat until the agent stops working. `--no-follow` returns as soon as the prompt is recorded.

The command line says "repository" where the app says "project", and "session" where the app says "chat".

## Naming a session

Wherever a command takes a session, give it one of:

- **A session's ID,** or the start of one.
- **A workspace's name,** for that workspace's newest open session.

## Repositories

| Command | What it does |
|---|---|
| `roundhouse repo add <path>` | Registers a local git repository |
| `roundhouse repo list` | Lists them |

## Workspaces

`ws` is short for `workspace`.

| Command | What it does |
|---|---|
| `roundhouse workspace create <repo>` | Makes a worktree on a new branch. `--name` names it, and `--base` branches from another ref |
| `roundhouse workspace list` | Lists workspaces. `--all` includes archived ones |
| `roundhouse workspace status <workspace>` | Prints the branch it's on, and the files it has changed |
| `roundhouse workspace diff <workspace>` | Everything its branch changes against its base |
| `roundhouse workspace rename <workspace> <title>` | Changes its title. Its name stays |
| `roundhouse workspace archive <workspace>` | Removes the worktree, and keeps the branch and history. `--force` archives even if that loses work |
| `roundhouse workspace restore <workspace>` | Brings back an archived workspace's worktree |

## Sessions

| Command | What it does |
|---|---|
| `roundhouse session new <workspace>` | Starts a session. Nothing runs until its first prompt. Takes `--model`, `--effort` and `--permission-mode` |
| `roundhouse session list <workspace>` | Lists a workspace's sessions |
| `roundhouse session set <session>` | Changes the model, effort or approval mode, on the running agent too |
| `roundhouse session rename <session> <title>` | Changes its title |
| `roundhouse session close <session>` | Stops its agent and hides it. The history stays |
| `roundhouse session reopen <session>` | Brings a closed one back |

`--permission-mode` is `plan`, or `bypassPermissions` for full access, which is what leaving it out gives. `roundhouse agents` lists the models an agent is offered, and the efforts of the one a chat starts on. `session new --agent claude` selects Claude, `--agent codex` selects Codex, and `--agent antigravity` selects Antigravity. Leaving out `--agent` uses the hub’s default.

## Talking to an agent

| Command | What it does |
|---|---|
| `roundhouse send <session> <prompt>` | Sends a prompt, then follows until the agent stops working. `--attach <path>` sends a file with it, and can be given more than once |
| `roundhouse attach <session>` | Follows a session's events. `--from` starts partway, `--until-idle` stops once the agent does, and `--json` prints events as JSON |
| `roundhouse log <session>` | Prints a range of a session's events and exits. Takes `--from`, `--limit` and `--raw` |
| `roundhouse cancel <session>` | Stops the agent's current turn. The session stays usable |
| `roundhouse compact <session>` | Asks the agent to compact now. `--keep` says what to keep |
| `roundhouse approve <session> <approval-id>` | Allows the tool's use the agent is waiting on, once, or approves its plan. `--deny <message>` refuses either, with a message to the agent, and a plan refused goes on being planned. Questions are answered with `answer` |
| `roundhouse answer <session> <approval-id> <question> <answer>` | Answers a question the agent asked, by its text. An answer that's an option's label chooses that option, and where several can be chosen, labels separated by a comma and a space choose each. Anything else is typed as the answer, where the question takes one |

## The hub

| Command | What it does |
|---|---|
| `roundhouse hub status` | Says which version the hub is, how many workers, running sessions and clients it has, and whether it was started by hand, by a client that needed one, or as a service. For a service, it says which release the hub would update to |
| `roundhouse hub stop` | Stops it, and any agents at work on it, which pick up their conversations at their next prompts |
| `roundhouse hub update` | Updates a hub installed as a service to the release `hub status` says it would update to. It downloads and checks it, places it beside the one it runs, and restarts on it, and agents at work keep going |
| `sudo roundhouse hub install --user <name>` | On a Mac or Linux, makes this build's hub a service that starts at boot as that user, with nobody logged in, and starts it. Run it from where the hub's files are, and run it again to move the service to another build |
| `sudo roundhouse hub uninstall` | Removes the service, which stops its hub. Its data stays in its user's data directory |

Quit the app before stopping the hub, since the app starts a new one as soon as this one stops. A hub installed as a service stays stopped until the machine starts again, or until `hub install` runs again.

## Paired devices

| Command | What it does |
|---|---|
| `roundhouse phone pair` | Prints a QR code and single-use pairing link for another computer or phone, and waits for it to connect |
| `roundhouse phone list` | Lists paired devices and when each was last seen |
| `roundhouse phone revoke <device>` | Revokes a paired device by name or ID prefix and closes its connections |

**Pair a device with this Mac…** in the app's hub switcher generates a link and QR code the same way. See [Connect to another machine's hub](../guides/connect-another-hub.md).
