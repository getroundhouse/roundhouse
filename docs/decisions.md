# Decisions

What's been decided, and why. Questions still open are GitHub issues, labelled `decision` and `design`.

| Area | Decision | Why |
|---|---|---|
| Direction | Build our own rather than adopt or fork | See `README.md` |
| First milestone | A local-only desktop app, done when the user chooses it over any alternative for a full day of local work | Feel is the reason to build this, so it gets proven before remote, mobile or hosted work |
| Scope | Desktop, cloud and mobile are all in scope | Cloud is where the heaviest work happens |
| Web | Not a priority. Keep the path open, but don't test or build for it now | Desktop comes first, and nothing depends on a web client yet |
| Architecture | Hubs own state and serve the platform API. Workers run agents, terminals and git. Providers give any hub somewhere to run workers. Clients can connect to several hubs | One code path for local, self-hosted and cloud. Logs survive sleeping or dead sandboxes |
| Providers | A core hub capability, not a cloud feature. Custom adapters are supported, such as a hub on a Mac driving Incus instances. Adapters are executables over stdio to start, with gRPC possible later. Incus is the first external adapter, in milestone 2 | User requirement. Local is just the default provider |
| Sync | Ordered event log with range reads. Clients never hold a whole transcript | Long sessions are remote. CRDTs suit many people typing in one buffer, which isn't this product |
| Transport | Workers connect over any byte stream provided by the adapter. Phones reach private hubs through a relay | Keeps adapters simple, and phones can't always reach a hub directly |
| Hub availability | A hub is only reachable while its machine is awake. Persistent mobile access needs an always-on hub | Accepted by the user as the expected behavior |
| Product model | "The workspace is the unit of delegation; the branch and pull request are the unit of integration" | This mental model is the reason to build at all |
| Small features | Keep a ledger of them as a first-class artifact, in GitHub issues labelled `small-features` | They are the differentiator |
| Desktop UI | **GPUI with gpui-kit** | User's decision on 2026-11-06, ahead of final timing runs. See `docs/ui.md` |
| Subscription login | Users can bring a Claude subscription | The user reports Anthropic has settled this. The Agent SDK overview page read on 2026-11-06 still showed the older restriction, so a link to the confirmation should be added here |
| Client core | Rust, shared across clients where possible | Follows from GPUI, replacing the earlier shared TypeScript client idea |
| Name and assets | New name, no code or assets copied from Conductor | It's a new product, and this avoids legal and trademark problems |
| Working name | **Trestle**, a rail bridge carried by many closely spaced supports, with rail as the theme | The user's decision on 2026-11-06. It isn't cleared for going public, because a US trademark overlaps, and #8 tracks that |
| Framing | Conductor is an inspiration, not a spec. Docs describe this as a new product on its own terms, not as a clone or a replacement | The user's decision on 2026-11-06 |
| Conductor compatibility | Keep reading `.conductor/` settings and setting `CONDUCTOR_*` variables, as a compatibility feature, with no names of our own for now | The user's decision on 2026-11-06. Repositories already set up for it keep working. It can be revisited once the product has a name |
| Zed crates | Use the Apache-2.0 GPUI crates, not the GPL-3.0 ones | GPL is widely seen as incompatible with the App Store |
| GPUI mobile backend | Undecided: compare Zed's `gpui-pre-ios` with Longbridge's `gpui-pre-mobile` in the proof of concept | Both can be used under Apache-2.0. `gpui-pre-mobile` also covers Android |
| Transcript | The kit's `MessageScroller` and `TextView`, plus a small windowed element for large tool output, in `crates/transcript`. Chosen on 2026-11-06 in step 1.2, and the user can overrule it after trying it | It passes every bar, and it replaces about 3,300 lines of unreviewed custom text engine and shares one path with iOS. The custom transcript was smoother, and was deleted with the prototypes, so there's no longer a fallback on hand. See `docs/ui.md` |
| UI tests | The desktop app is tested through its real UI, by clicks and typing in the kit's headless windows, against a real hub process and real git, with only the agent faked | No test code in the product, no window on screen, and about 3 s for the whole core loop. It found bugs a screenshot can't, such as a click that silently did nothing |
| Workspace setup | Runs in the background once the worktree exists. Copies gitignored files matching `.worktreeinclude`, then `file_include_globs`, then `.env*`, and runs `scripts.setup` with the `CONDUCTOR_*` variables. A prompt sent meanwhile is recorded at once and delivered when setup finishes. Failure warns, and never blocks. Restoring runs it again | It reads Conductor's settings and variable names so repositories already set up for it work here. The matching is git's own, so patterns mean what they do in `.gitignore` |
| `Create PR` | Asks the agent to commit, push and run `gh pr create`, and the app shows the result from `gh pr view` | The agent knows why the changes were made, so it writes the better description |
| `option-ext` | Replaced by a small stand-in in `third_party/`, written from its public API | The real one is MPL-2.0, and it would ship through `zed-font-kit` and `dirs`. Three small methods aren't worth an exception to the no-copyleft rule |
| Driving agents | The worker runs the `claude` CLI directly from Rust over its stream-json protocol, with no SDK sidecar | The SDK wraps this same protocol, and everything the product needs so far works directly. See `docs/claude-cli.md` |
| Which agent binary | The user's installed `claude`, for now. Whether to pin and ship one is still open | Nothing to redistribute, and it picks up the user's login and settings. The price is that a CLI upgrade can break the adapter, which `tools/smoke/capture_claude_fixture.py` and the contract tests are there to catch |
| Wire format | Newline-delimited JSON, a version handshake, and normalized events with the raw line stored beside them | See "Wire format and event log" in `docs/architecture.md` |
| Workers | One process per workspace | Isolates crashes and matches the sandbox model. About 8 MB each |
| Client connection | Blocking calls and channels with no async runtime in `client-core` | Keeps GPUI's executor out of it, and leaves a native mobile client possible |
| Rust standards | `docs/rust.md`, agreed by the user on 2026-11-06. New code follows it, and existing code moves over through the `Adopt the Rust standards` milestone | See "Rust standards" below |
| Locks | `parking_lot::Mutex` for locks not held across `.await`, once `docs/rust.md` is adopted | The user's decision on 2026-11-06. 80 of the 89 `unwrap`s and `expect`s outside tests were lock results, and `#[expect]` on each would be noise. `.lock()` can't fail, so it needs no `unwrap`. It's MIT OR Apache-2.0 and already in `Cargo.lock`, and `std::sync::nonpoison` is still unstable on 1.98.1. The trade-off is that after a panic, other tasks see whatever state it left rather than a poisoned lock |
| Backend errors | `ApiError` stays the error for hub and worker operations and across the wire, with `NameTaken` and `NeedsForce` split out of `Conflict`, and `hub::store` given its own enum | The user's decision on 2026-11-06. See "Rust standards" below. A `Conflict` that's too broad can offer a forced archive that can't help, and the store's errors lose their source and aren't logged |
| Archiving | Removes the worktree and keeps the branch, following it if the agent renamed it. Refuses if there are uncommitted changes, or commits on no branch, unless forced. Ignored files such as `.env` and `node_modules` go with the worktree | Committed work can't be lost, and uncommitted work can't be lost by accident. Snapshotting uncommitted work could replace the refusal in the feel pass. Whether ignored files such as `.env` should survive is for the user to decide, in #61 |
| Bake-off | Deleted on 2026-11-06, and removed from git history before the first push. Its bars and lessons are in `docs/performance.md`, its results in `docs/ui.md`, and its test server in `mobile-poc/test-server/` | The user's decision: it had served its purpose. The custom transcript and the terminal element went with it, so a terminal starts fresh, and rejecting the kit's transcript would mean rebuilding one |
| Glob imports | `use gpui_kit::*` goes, once `docs/rust.md` is adopted | The user's decision on 2026-11-06. It removes the `#[test]` trap for good. The cost is long import lists in 8 files |
| Size limits | 400 lines per file and 60 per function | The user's decision on 2026-11-06. They flagged 12 files and 13 functions, which is few enough to fix. Lines here run to 120 columns, so they're a little looser than they sound |
| Docs and project management | Each doc has one clear purpose. GitHub tracks what's left: issues for questions, risks, gaps and tasks, and milestones for the build order | The user's decision on 2026-11-06, which replaced a status doc in the repo |
| New chats | A new chat keeps the model and how edits are handled, but not Plan or Full access | Plan belongs to the task at hand, and Full access should be chosen on purpose each time. This is a suggestion, for the user to judge |
| Closing chats | Closing a chat stops its agent and keeps the history. Closing the last one starts a new one rather than leaving nothing, and a workspace's first chat is found or made by the hub | Two clients opening a workspace at once share one chat |
| Sidebar dot | It speaks for the open chat that most needs attention | A chat waiting on approval is visible from anywhere |
| Answering questions | While the agent waits on a question, what's typed in the composer is the answer. The card says so | A plain Allow button would throw the question away |
| Chat shortcuts | `Cmd-T` for a new chat, `Cmd-W` to close one, `Ctrl-Tab` and `Ctrl-Shift-Tab` to move between them | They're what tabs use elsewhere |
| Model picker | Offers aliases: `default`, `fable`, `opus`, `sonnet` and `haiku` | It follows the CLI's newest of each without a release here |

## Rust standards

The user agreed `docs/rust.md` on 2026-11-06, and it hasn't been adopted yet. The `Adopt the Rust standards` milestone tracks that. It started from the Rust conventions of another project the user works on. That project is private, so the rules were rewritten for this repo rather than copied, and it isn't named here.

**Kept close to the source:** the lint set, the 400 and 60 line limits, no logic in `lib.rs`, newtype IDs, typed errors in libraries, one integration test binary per crate, and how docs and tests are written.

**Changed for this repo:**

| Source rule | Here | Why |
|---|---|---|
| Errors become transport responses in one place, and domain code knows nothing of them | The hub and worker can return `ApiError` directly, except `hub::store` | `ErrorCode` is a closed set of meanings, not a transport status. Both places that branch on an error depend on a condition from across a process boundary, so a second enum in the hub would only mirror it. `hub::store` has its own enum because `ApiError` would throw away the SQLite error |
| `anyhow` only in a binary's startup | Anywhere in a binary | A binary's errors only reach a person or a log |
| Warnings fail CI | `-D warnings`, run by hand | There's no CI yet |
| A shell script checks file length | A policy test | `tools/policy` already fails `cargo test` on this kind of rule, and it runs anywhere |
| `missing_docs` on crates others build on | Off | `protocol` alone would need 420 docs, most repeating a field's name |
| Non-trivial SQL in `.sql` files | Inline in `hub::store` | It's a few dozen statements through rusqlite, with nothing to check them at compile time |
| Never silence a lint across a module or crate | Test programs in `examples/` can | `fake_claude` and `look` are test code, but Clippy doesn't treat them as tests |
| Errors never asserted by message | Except text the user sees | That text is what a UI test is about |

**Added for this repo:** protocol compatibility rules, `parking_lot` over `std::sync::Mutex`, no glob imports, process groups, and GPUI.

**Left out, because this repo doesn't have the problem they solve:** build system check crates, shared build caches, automated dependency updates, compile-time checked SQL, Docker services for tests, WebAssembly builds and generated protobuf forms. nextest is also left out. It isn't installed here, `cargo test` works, and it doesn't run doc tests. It's worth revisiting if tests start to trip over shared process state.

**How it was checked:** the lints ran on a clean copy of the code as of the commit that added workspace setup, in the Cargo config `docs/rust.md` describes. A scratch crate confirmed that `let … else` isn't a wildcard match, that a stale `#[expect]` warns, that Clippy flags a bare helper in `tests/` but not one under `#[cfg(test)]`, that `prelude::*` and `super::*` in tests are allowed, that `disallowed-types` catches `std::sync::Mutex`, and that a `parking_lot` guard across `.await` is flagged. Cargo refused crate lints alongside `[lints] workspace = true`, and `std::sync::nonpoison` is still unstable on 1.98.1.

**Not checked:** no code has changed. The `parking_lot` swap, a clean run under `-D warnings`, the tests after `cargo clippy --fix`, and the Linux Docker build with a pin are all still to do.
