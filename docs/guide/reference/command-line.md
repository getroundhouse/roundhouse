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

`--permission-mode` is `plan`, or `bypassPermissions` for full access, which is what leaving it out gives. `roundhouse agents` lists the models and efforts an agent is offered.

## Talking to an agent

| Command | What it does |
|---|---|
| `roundhouse send <session> <prompt>` | Sends a prompt, then follows until the agent stops working. `--attach <path>` sends a file with it, and can be given more than once |
| `roundhouse attach <session>` | Follows a session's events. `--from` starts partway, `--until-idle` stops once the agent does, and `--json` prints events as JSON |
| `roundhouse log <session>` | Prints a range of a session's events and exits. Takes `--from`, `--limit` and `--raw` |
| `roundhouse cancel <session>` | Stops the agent's current turn. The session stays usable |
| `roundhouse compact <session>` | Asks the agent to compact now. `--keep` says what to keep |
| `roundhouse approve <session> <approval-id>` | Allows what the agent is waiting on. `--deny <message>` refuses, with a message to the agent |
| `roundhouse answer <session> <approval-id> <question> <answer>` | Answers a question the agent asked |

## The hub

| Command | What it does |
|---|---|
| `roundhouse hub status` | Says which version the hub is, and how many workers, running sessions and clients it has |
| `roundhouse hub stop` | Stops it, and any agents at work on it, which pick up their conversations at their next prompts |

Quit the app before stopping the hub, since the app starts a new one as soon as this one stops.
