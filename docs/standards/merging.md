# Merging

Which checks a pull request needs before it's pushed, how it merges through the queue, what to do about a flaky test, and when a break in the worker protocol can merge. `docs/standards/pull-requests.md` says how a PR is written, and `docs/standards/stacks.md` how a change is split into several.

## The full checks

Run the full checks once per PR that changes code, just before pushing. A PR that only changes Markdown or the marketing site, which is `site/` and `tools/site/`, needs none of them, since none of them reads either. Warnings count as failures. Clippy runs twice: on every target, and then on the product's code alone, as it ships, since the `testing` features that the tests turn on can hide an import or an item only the tests use. CI runs them too, on Linux for every PR that changes more than those, and on a Mac only when dependencies change, so it doesn't replace running them here. Windows has a workflow of its own, which lints the product's code as built for it for every PR that changes Rust, builds the hub and the CLI, and starts and stops a hub there. It doesn't block a merge, and it's the only place anything runs on Windows. On Linux, `--workspace` needs the packages `.github/workflows/ci.yml` installs for GPUI, and the gallery doesn't run:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --locked -- -D warnings
cargo test --workspace --locked
```

## The merge queue

PRs merge through GitHub's merge queue, squashed, once CI's `checks` job passes on the PR merged onto `main` and whatever is ahead of it in the queue, so only each PR's final state reaches `main`. Add a PR to the queue only when the owner asks, but for a fix to a flaky or broken test, below. A stack goes to the queue with `gh stack merge`, which takes every PR up to the one it names, each to land as its own commit. PRs that land together land in one push, so `main` publishes one build for them. A PR that conflicts with what lands ahead of it is taken out of the queue with those above it, and nothing puts them back: rebase it, and once its CI passes, queue it and those above it again with `gh stack merge`. So a build with only part of a stack can still be published. A stack can merge each PR as it's ready. GitHub takes no push to a queued PR's branch, so to change one, take it out of the queue with GraphQL's `dequeuePullRequest`, which also turns off its auto-merge, then push, and queue it again once its CI passes. Between commits, `cargo check` and the tests for what changed are enough. Once the full checks have passed, a rebase onto `main` that applied cleanly needs only fmt, clippy, `cargo test -p policy` and the gallery before pushing, since the queue runs the tests on the PR merged onto `main`, and `main` often moves while the full checks run. Clippy builds every target, which catches what a clean rebase can still break, and the gallery runs nowhere else. A rebase whose conflicts were resolved by hand needs the full checks again.

## A flaky test

For a flaky test, repeat it only enough to see how often it fails, find the cause by reading the code, then repeat it only enough to confirm the fix. Put the fix in a PR of its own from `main`, and when it changes only tests and the code that supports them, add it to the queue yourself once its CI passes, and say you did. Until it merges, the test fails PRs that didn't break it and throws them out of the queue, so it shouldn't wait for the owner to notice it. Making a test wait for the right thing is a fix. Skipping it, deleting it or loosening what it checks isn't, and waits for the owner like any other change.

## A break in the worker protocol

A PR that breaks the worker protocol merges only once each platform whose apps update in steps, which are macOS and Linux, has published a release that includes the previous break. The app's update crosses one break at a time, as `docs/principles/protocol.md` says, so it needs a published release between every two breaks to step to. Until then, CI's `steps` job, which runs `tools/update-steps`, fails the PR, in the queue too. Queue it again once those releases are published.

A build can also say the oldest release it updates from, its floor, since its hub can't migrate the database of a release before that one, as "The database" in `docs/principles/state.md` says. The floor moves only at a break in the worker protocol, to a release the update feed keeps for apps to step to on every platform, and, where apps step, to none newer than the last release whose hub doesn't speak the break's oldest workers. An app from before releases said their floors then steps through it as it steps through the break. The `steps` job fails a floor that breaks either rule, and `tools/package/publish.py` refuses one that got past it, as it does a break. *Unconfirmed.*

A published break in the worker protocol is fixed forward, not reverted. An app can have one worker on the break's version and an older one still at work. That app would take the revert as an update, and its hub would stop the newer worker, with its agents. The exception is a break that got past the `steps` job. `tools/package/publish.py` refuses to publish it, and every release after it, on a platform that has no release to step to, and only a revert gets that platform publishing again.
