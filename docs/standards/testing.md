# Testing

Where each kind of test goes, what a test runs in, which is its hub, its clocks and its environment, and how a test is written. The commands are in `docs/reference/commands.md`, and what a person has to judge by hand is in `docs/standards/manual-checks.md`. Known gaps are GitHub issues.

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
| An agent's CLI | That a version of it does what Roundhouse needs, as `docs/reference/conformance.md` says | `crates/agents/src/conformance/` |
| The real CLI | That the fake agent is still honest. By hand, since it costs money | `tools/smoke/` |

- **A rule the hub decides by itself is a unit test in the hub,** on a hub from `hub_with_stand_in_workers`, whose stand-in answers as a worker would and says what it was asked.
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

- **A test doesn't wait out one of the product's timers,** which it would on every run. One in the product's own process moves a test clock: tokio's, paused, or GPUI's, with `advance_clock`. One that starts a process has it take shorter timers, as below. Only a dependency's timer that a test can neither move nor shorten is waited out.
- **A real hub and its workers take their timers from the environment,** as `hub::Timers` and `worker::Timers` say, only in a build with the `testing` feature. A timer a test would otherwise sit through goes in one of them.
- **Tests shorten the timers that only make them wait, and keep the product's time limits,** which `FOR_TESTS` does for every e2e and UI test's hub. A limit guards against a slow process, which one on a busy runner is.
- **Only a test about a limit changes it.** To see the limit hold, it shortens it, through `Stack::start_with`. To see that something didn't wait for the limit, it lengthens it far past its own wait, as `patient_with_workers` does, rather than timing the product's value.
- **The window's timers run on GPUI's clock, which a test moves.** A wait by the system's clock, such as how long quitting waits for the hub, takes a global a test sets.

## Environment

- **A process a test starts runs in a `process::testing::WholeEnvironment`, not the test's own,** so nothing of whoever runs the tests reaches it: a token, a `GIT_DIR` from a git hook, a login shell's setup, a coding agent's variables. It keeps `PATH` and where temporary files go, and the test sets the rest. `Stack` and `hub_environment` make one, and a hub a client starts gets its `Daemon`'s, by `in_environment`.
- **A real hub takes where it calls GitHub from the environment,** as `hub::GithubConfig::from_env` says, so a test points it at `hub::GithubStandIn`.
- **A test makes and reads its repositories with `process::testing::git`,** which reads none of the person's git config.
- **The worker's unit tests give what the product's code starts `testing::given`,** the same environment, never `Environment::NONE`, which `tools/policy` refuses there.

## Writing tests

- **Test each guarantee at the narrowest boundary that proves it.** Assert what can be observed: returned values, stored state, effects, and state left unchanged after a refusal. Separate setup, the action and assertions with blank lines.
- **Each level tests its own guarantees.** A behavior tested at one level appears at a higher one only as one example showing the wiring.
- **A change extends the test that covers what it changes.** One more case of a rule a test already covers is a row in its table or an assertion beside the others: a field added to a record, a kind added to an enum. A new test is for a guarantee no test covers. A test that would start as another does, and differ in one input or assertion, is a case of that one, so they share the cost of their setup, which matters most where a test starts a process.
- **What several callers share is tested once, where it's defined.** A generic type's rules are tested on an instance made up for the test, and a helper's against the helper. Each caller gets one example of the wiring, and tests only what it adds. So a rule that holds for every agent, column or kind of record is one test that goes through them all.
- **A few unit tests sit beside their code.** When they grow, they move to `foo/tests.rs`, declared with `#[cfg(test)] mod tests;` in `foo/mod.rs`.
- **A crate keeps one integration test binary,** `tests/<name>/main.rs` with its modules beside it, since each file directly in `tests/` links separately.
- **Integration test files start with `#![cfg(test)]`.** Clippy only lets code unwrap under `#[test]` or `#[cfg(test)]`, so without it a helper in `tests/` gets flagged. A fixture shared beyond one file goes in a module declared `#[cfg(test)]`.
- **A program a test runs in place of another is the example of a package of its own, in `tools/stand-ins/`,** which depends on only what the program uses, so an edit to the crate whose tests run it doesn't build it again. It's test code, so it can unwrap, with one `#![expect(…, reason = "…")]` at the top. It's an example rather than a bin, since a bin built alone doesn't take the tests' features, and the product's own checks build every bin. Its name is unique in the workspace, since `--example` builds every member's example of that name.
- **A test builds a program it runs with `process::testing::workspace_build`,** which is `cargo build --workspace`, naming it with `--example`, and a `--bin` only beside one. Built with the whole workspace's features, the program is among what the build of the tests made, so it compiles nothing. Built any other way, or in a run of one package's tests, such as `cargo test -p agents`, Cargo compiles a second copy of every crate whose features differ. `Cargo.toml` and `workspace_build` say what else that rests on.
- **A test passes under `cargo test` and under nextest,** which CI runs. nextest gives each test a process of its own, where `cargo test` runs a binary's tests in one, so a `OnceLock` in a test runs once per test under nextest. Work that should happen once per run, such as building a helper, takes a file lock in `CARGO_TARGET_TMPDIR`, keyed by `NEXTEST_RUN_ID`. nextest skips doc tests, so CI runs those with `cargo test --doc`.
- **What only tests read or set goes behind the crate's `testing` feature,** which the crate turns on for its own tests by naming itself in `[dev-dependencies]`, so the product never builds it.
- **A test makes a session, workspace, event, subagent or child worktree from `protocol::fixtures`,** behind protocol's `testing` feature, and sets only what it's about, with struct update syntax. A field added to a record then changes one builder rather than every test that makes one. A test of how a record reads on the wire, such as one from an older peer, spells out its JSON instead.
- **A protocol break is tested against the version before it.** Its PR spells out the previous version's wire form of each message it changes, in `crates/protocol/tests/compatibility/`, and tests that the hub still reads and answers it. *Not built:* CI running a turn with older releases' workers.
- **Laws, invariants and round trips are property tests** with fixed seeds that shrink a failure to its smallest case. An example beside a law pins only what the law doesn't.
- **Errors are asserted by variant or code, not by message.** The exception is text the user sees, which is what a UI test is about.
- **A test takes under 5 s in CI's test job, and CI fails one that takes over 10 s,** even if it passed, since every PR and the merge queue wait for the tests. The gap leaves room for a slower runner or a busier moment. There are no exceptions: a test that's big on purpose is made as small as still catches what it's for, or checks the same thing a cheaper way.

Tests that can't fail prove nothing. When a behavior matters, break it on purpose once and confirm a test notices. Undo the break with an edit, not by copying a saved file back: some `cp`s, such as the one in Homebrew's `uutils-coreutils`, keep the saved file's older modification time, so cargo doesn't rebuild and goes on testing the broken code.
