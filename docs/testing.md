# Testing

Where each kind of test goes, and what a test runs in: its hub, its clocks and its environment. How to write a test is in `docs/rust.md`, the commands are in `AGENTS.md`, and what a person has to judge by hand is in `docs/manual-checks.md`. Known gaps are GitHub issues.

## Levels

| Level | Tests | Where |
|---|---|---|
| A module | Its own rules, without a process or a socket. One that runs a program tests its rules without it, and starting, watching and stopping one against a stand-in | Unit tests |
| The hub | What the hub decides by itself, with stand-ins for its workers | Unit tests in `crates/hub/` |
| The agent adapter | Normalizing the CLI's captured output | `crates/agents/tests/` |
| The whole backend | What spans processes: restarts, crashes, a real worker, with a fake agent | `apps/daemon/tests/` |
| The window | Clicking and typing through to what's shown | `apps/desktop/tests/` |
| How each screen looks, and the app icon | That each still matches its approved image | `apps/desktop/tests/ui/gallery/` |
| The repo's rules | Dependencies, licenses, file length and lint inheritance, against the real dependency graph | `tools/policy/` |
| The guide's tool | That it publishes each page and refuses a link, heading or picture that isn't there, against made-up guides. CI's `guide` job builds the real one | `tools/guide/tests/` |
| The kit's tool | That it makes the copy of gpui-kit and keeps its patches in step with it, against stand-ins for upstream's repository and this one | `tools/kit-sync/tests/` |
| Publishing updates | That a release signs only a feed it computed, finishes after stopping partway, and reads back as apps would, against a directory standing in for the bucket | `tools/package/test_publish.py` |
| An agent's CLI | That a version of it does what Roundhouse needs, as `docs/conformance.md` says | `crates/agents/src/conformance/` |
| The real CLI | That the fake agent is still honest. By hand, since it costs money | `tools/smoke/` |

- **A rule the hub decides by itself is a unit test in the hub,** on a hub from `hub_with_stand_in_workers`, whose stand-in answers as a worker would and says what it was asked. An e2e test is for what spans processes, and for one example of a rule's wiring.
- **The window is tested through its real UI,** by clicking and typing in the kit's headless windows, which keeps test code out of the product and catches what a picture can't, such as a click that does nothing.
- **A window test opens on a fake hub, with `Ui::open_on`, unless it's about what a hub, a worker or an agent does.** What the window draws of what a hub holds needs none of them, and real ones make it slower and give it more to race. On a real hub, only the agent is faked.
- **The fake hub answers by the hub's own rules,** so it sends only what a hub could. It calls the rule in `protocol` that the hub calls. Where the hub's store does it in SQL, `protocol` holds the rule behind `testing`, and the store's tests hold the SQL to it.
- **The fake hub is in `client_core`, behind `testing`, not beside the gallery,** which only a Mac builds, so a protocol change it doesn't follow fails CI on Linux.
- **The gallery draws each screen from the fake hub's fixed data,** since a real hub's IDs, times and timing differ from run to run, and a picture has to come out the same. What a picture shows, and how one is accepted, is in `docs/spec/README.md`.

## Waiting

- **A test waits with the `waiting` crate** until what it waits for holds, failing at a time limit, or, to see that something doesn't happen, for a bound in which it could. `clippy.toml` refuses a sleep anywhere else, so one the product, a stand-in or a paused clock needs `#[expect]`s the lint and says why.
- **A window test waits for what it reads next.** A chat's record and its events reach the window apart, so a chat can be idle before the last of its turn is drawn. `Ui::wait_idle_with` waits for both, and `Ui::settle` plays only the frames the app asked for.
- **A process a test starts never stays up for a set time,** which a busy machine can outlast. It waits for a file the test makes, as the fake agent's `-until` commands and a `Gate` do. A script polls no more often than every 0.05 seconds, and one that runs until it's stopped sleeps 30 or more. `tools/policy` refuses a shell `sleep` between the two.

## Timers

- **A real hub and its workers take their timers from the environment,** as `hub::Timers` and `worker::Timers` say, only in a build with the `testing` feature. A timer a test would otherwise sit through goes in one of them.
- **Tests shorten the timers that only make them wait, and keep the product's time limits,** which `FOR_TESTS` does for every e2e and UI test's hub. A limit guards against a slow process, which one on a busy runner is.
- **Only a test about a limit changes it.** To see the limit hold, it shortens it, through `Stack::start_with`. To see that something didn't wait for the limit, it lengthens it far past its own wait, as `patient_with_workers` does, rather than timing the product's value.
- **The window's timers run on GPUI's clock, which a test moves.** A wait by the system's clock, such as how long quitting waits for the hub, takes a global a test sets.

## Environment

- **A process a test starts runs in a `process::testing::WholeEnvironment`, not the test's own,** so nothing of whoever runs the tests reaches it: a token, a `GIT_DIR` from a git hook, a login shell's setup, a coding agent's variables. It keeps `PATH` and where temporary files go, and the test sets the rest. `Stack` and `hub_environment` make one, and a hub a client starts gets its `Daemon`'s, by `in_environment`.
- **A real hub takes where it calls GitHub from the environment,** as `hub::GithubConfig::from_env` says, so a test points it at `hub::GithubStandIn`.
- **A test makes and reads its repositories with `process::testing::git`,** which reads none of the person's git config.
- **The worker's unit tests give what the product's code starts `testing::given`,** the same environment, never `Environment::NONE`, which `tools/policy` refuses there.
