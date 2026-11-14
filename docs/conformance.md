# Conformance

How a version of an agent's CLI is checked against what Roundhouse needs of it, by playing scenarios through its adapter, and how to play them.

A scenario is the same whatever it's played against: the steps a driver takes, driving the agent through `AgentProcess` as the worker does, and checks of what the adapter made of the trial, in Roundhouse's terms, such as a command call's `CallKind` rather than the CLI's name for its tool. Each agent's binding gives each scenario a script, which says what the scripted model replies. A trial plays a scenario once, against one of two models:

| Against | What the CLI talks to | Costs | Its capture |
|---|---|---|---|
| The scripted model | A stand-in for the model's API on 127.0.0.1, whose every reply the script makes | Nothing, and it goes the same way every time | A fixture, which `cargo test` replays |
| The real model | The real API, on the binding’s selected small model | Billed by the provider | Never kept, since it says what the account is and has used |

A scenario played against both checks that the script does what a real model does, closely enough that the checks hold on both. One only a script can make happen, such as a model that thinks on cue, is played against the scripted model alone.

## Playing them

```sh
cargo run -p agents --example conformance                    # Every scenario, against the scripted model, on the CLI the hub would find
cargo run -p agents --example conformance -- --paid          # Then each that plays there, against the real model
cargo run -p agents --example conformance -- --save          # Keeps each passing scripted trial's capture as its fixture
cargo run -p agents --example conformance -- --out .context/conformance plan_approved   # Keeps a scripted trial's fixture, failed or not, and what the model was asked
cargo test -p agents --test claude conformance               # Replays the fixtures through the adapter, with no CLI
```

`--cli PATH` plays on another CLI than the one the hub would find, such as a version to qualify, and both models' trials play on the same one. Names after the options play only those scenarios.

## Captures and replays

A trial captures each launch of the CLI, with what it was launched with, each line the adapter writes to the CLI and reads from it, in the order it did, and each exit. The adapter tells them to a tap the trial gives it, through its `spawn_tapped` entry point, which only a `testing` build has. A step that waits for something only the CLI's world shows, such as a file a command makes as it starts, adds where it saw it. A fixture is a scripted trial's capture, with what changes every trial though nothing else does made the same: the scratch directory is named `/scratch`, each UUID is numbered in the order it first appears, with those of each kind of request the adapter makes by itself in a range of their own, Claude’s reported times are zeroed and its plan files are named by order. Codex captures keep native durations and terminal IDs. A Claude fixture captured again therefore differs only where the CLI or the adapter did something else, which includes where and how often the adapter asks by itself, and what the CLI's context held when it answered.

A replay plays the same scenario through the adapter's own process code, with a stand-in for the CLI: a script that hands the adapter's arguments, stdin and stdout over to a replayer, which writes what the CLI wrote. Wherever the adapter wrote a line in the capture, the replayer waits until it writes the same line again, but for the IDs it makes afresh each time, such as a prompt's, which it maps to the ones in the capture. What the adapter asks by itself, as when a turn ends, it answers whenever it comes, as the CLI answered it in the capture. A step that waited on the world waits until the replayer has written all that came before where it saw it.

So a replay fails when a scenario's check doesn't hold of what the adapter made of the CLI's output, and when the adapter launches the CLI with other arguments, writes a line the capture doesn't have, doesn't write one it does, or writes one otherwise.

## Claude's binding

`claude::conformance` has Claude's scripted model, a stand-in for the Messages API, each scenario's script, and what a replay needs to know of stream-json. A trial plays on Haiku, the cheapest model, by its full ID, since by its alias a plan comes from Sonnet, as the plan-mode row of `docs/claude-cli.md` says, and at the lowest effort.

A scripted trial's CLI is kept from this machine:

- It runs under macOS's `sandbox-exec`, so a scripted trial needs a Mac. The sandbox lets it reach the scripted model's port and nothing else, not even a Unix socket, such as another Claude session's, and it can't make one of its own. It can neither look up, list, read nor write anything in this machine's home, the home itself included, but for reading the CLI. Outside the home it can read what this machine lets it, such as the managed settings an administrator puts there, which it still follows, and write what this machine lets it, such as its scratch directory.
- The example plays scripted trials in a process of its own, whose environment holds only what the CLI and its tools need to run, such as `PATH`, and it refuses any other.
- Its home and `CLAUDE_CONFIG_DIR` are a scratch directory, and so is its project, a git repository outside every checkout, so it finds no repository's instructions. It signs in with a key that's no one's, and `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC` turns off what it fetches and reports as it starts.

A trial against the real model isn't sandboxed. It uses this machine's login, settings and environment, but for what the adapter takes out, and removes the conversation and the plans it leaves in `~/.claude`. With `ROUNDHOUSE_CONFORMANCE_TOKEN` set to a token `claude setup-token` made, it signs in with that, and keeps to a home and config directory of its own, though it still has the rest of this machine's environment.

## Codex's binding

`codex::conformance` uses the same scenarios and replay driver, with an HTTP Responses stand-in and native app-server JSON-RPC captures. Select it with `cargo run -p agents --example conformance -- --agent codex`; `--save` records synthetic fixtures and `--paid` also uses the worker machine's Codex sign-in, on `gpt-6-luna` at Low effort. Real captures are never saved, and a real trial deletes its own native thread when it finishes.

Scripted trials use a scratch project and separate `HOME` and `CODEX_HOME`. On macOS, `sandbox-exec` denies outbound traffic except the scripted model's loopback port, and denies the user's home except the installed CLI's directory. Fixtures cover native text, tools, reasoning summaries, resume, cancellation and plan approval; replay also checks that a plain final reply in plan mode cannot bypass approval.

## After a CLI upgrade

Play the upgraded agent's scenarios against its scripted and real model with `--agent claude` or `--agent codex`. Record native behavior changes in `docs/claude-cli.md` or `docs/codex-cli.md`, and adapt the binding when Roundhouse should follow them. For a Claude upgrade, also run `tools/smoke/capture_claude_fixture.py`, since its other fixtures come from that tool. Once the scenarios pass, `--save` refreshes synthetic conformance fixtures, and `cargo test -p agents` checks both adapters without a paid model.
