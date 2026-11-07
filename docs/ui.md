# Why GPUI and gpui-kit

The choice of desktop UI technology, and the evidence behind it. The numbers come from prototypes that have since been deleted.

**The user chose GPUI with gpui-kit on 2026-11-06.** This was a direct decision, not the outcome of the bake-off rule: the bake-off was closed before its final runs. The 42 final timing runs, the Iteration test and the side-by-side manual comparison did not happen, and the Tauri prototype was stopped unfinished.

### What the dev runs showed

These are noisy numbers from a busy Mac with unfinished code, using only runs where the two apps didn't overlap. They are not final results.

| Measure | GPUI | Tauri |
|---|---|---|
| Frame rate | 120 Hz | 120 Hz, using a private WebKit API |
| `parallel` p99 frame (bar 18 ms) | 8.5 to 9.0 ms | 10 ms |
| Input latency p95 (bar 50 ms) | 15.8 ms | 17 ms |
| Terminal p99 frame, 266 MB of output | 8.5 ms | 11 ms |
| Diff open | 141 ms | 452 ms |
| Cold start, latest clean runs | 175 to 205 ms | 323 to 362 ms |

- Neither app showed a significant gap on the frame and input bars where they could be compared.
- `scroll` and `resources` never had a clean comparison.
- **The apparent GPUI freezes were not evidence of a difference.** A covered window stops GPUI's frame source, and the prototype's analysis attributes those freezes to that, with high confidence for some and moderate to low for the early `diff` and `open` ones. Most Tauri runs had occlusion detection turned off through a private API, so Tauri kept drawing while covered and the two weren't measured under the same conditions.
- WKWebView caps `requestAnimationFrame` near 60 Hz by default. According to the prototype's code comments, there is no public API to change this.

### Trade-offs accepted with GPUI

- **Mobile immaturity:** all GPUI mobile backends are experimental. Zed's iOS backend is weeks old, and Android depends on a community backend.
- **API churn:** GPUI is pre-1.0 and gpui-kit pins exact snapshots.
- **More code to own:** the prototype was about 1.7x the size of Tauri's when last compared (6,054 vs 3,598 lines), and has a custom terminal built on `alacritty_terminal`.
- **A custom text renderer, at first:** the prototype's transcript was custom, as a judgment call. The transcript test below went on to choose the kit's instead.
- **Unverified manual checks:** IME, VoiceOver, trackpad feel and selection across messages haven't been hand tested. See `docs/manual-checks.md`.

### Prototype results

Dev runs on an M4 Max at 120 Hz, with the window uncovered.

| Scenario | Result | Conditions |
|---|---|---|
| `parallel` at 15k | p99 8.4 ms, worst frame 18.6 ms, input p95 15.8 ms | Final binary, no keep-visible flag |
| `diff` | p99 8.4 ms, worst frame 17.7 ms, open 180 ms, all 20 comments placed | Final binary, keep-visible flag |
| `scroll` at 15k | p99 8.4 ms, worst frame 25.8 ms, 0 px drift on 100 jumps, jump p95 216 ms | Earlier binary, keep-visible flag |
| `open`, all tiers | Worst 226 ms | Earlier binary |
| `terminal`, 266 MB | p99 8.5 ms, worst frame 16.6 ms, 11 ms lag | Earlier binary, dev flag |
| `resources` | 0.0% idle CPU, 415 MB footprint after idle | Earlier binary, keep-visible flag |

- Each result comes from a single run, with no repeats.
- `scroll` wasn't run on `s` or `xl`, and has no clean run on `m`.
- Nobody has used the app by hand. IME, dictation, VoiceOver, trackpad feel and the real clipboard are untested. Find in transcript and image paste aren't implemented.
- **Measurement lesson:** a covered window looks exactly like a freeze, so frame data needs a window visibility log alongside it.

### What happened to the bake-off code

It was deleted on 2026-11-06, and removed from git history before the first push, including the prototypes, the harness and the raw results. These numbers can't be rerun. The bars and lessons are in `docs/performance.md`, and the test server is in `mobile-poc/test-server/`.

## Transcript decision

Step 1.2 compared the prototype's custom transcript with one built on the kit, in the same scenarios.

| Across phases, `scroll` on `l` | Bar | Custom | Kit |
|---|---|---|---|
| p99 frame | 18 ms | 8.5 ms | 13.5 to 16.6 ms in 3 runs |
| Frames over 25 ms | 1% | 0% | 0.27 to 0.31% |
| Worst frame | None | 17 ms | About 135 ms |
| Anchor drift on jumps | 1 px | 0 px | 0 px |

**Why the kit:**

- It meets every bar. On real content it nearly matches the custom transcript, with p99 at 8.5 ms for both.
- **Giant rows were the argument for going custom, and they turned out fine.** A windowed element of about 70 lines flings through a 2.2 MB row at p99 8.4 ms. Even one plain text element holding it all stays under the bars, at a lower frame rate.
- It replaces about 3,300 lines that have had no review and no tests, and that still need bidi, accessibility and find.
- Markdown, tables, code highlighting, links and selection across messages come with it.
- It's the same code on iOS, where `MessageScroller` already ran in the proof of concept.

**What it costs:**

- It's measurably less smooth at the extreme. The custom transcript had no frames over 25 ms.
- One stall of about 130 to 160 ms near the first scroll on synthetic fixtures, with the cause unknown.
- About 90 MB more footprint, and about 130 ms more cold start.

Nobody has tried either by hand, so this could still change. The row renderer sits behind `crates/transcript`. The custom transcript was deleted with the prototypes, so changing course would mean rebuilding it.

## Alternatives

Other parallel agent apps, and what they're built on.

| Project | License on GitHub | Stars | Installed app |
|---|---|---|---|
| Orca | MIT | ~118k | Electron, 725 MB |
| Paradev (formerly Superset) | MIT | ~16k | Electron, 600 MB |
| Paseo | "Other" (custom) | ~23k | Electron, 520 MB |
| Emdash | Apache-2.0 | ~6k | Not installed |

## UI technology

### Zed's ideas

| Idea | Verdict |
|---|---|
| Rust core | Yes. The same binary runs on a Mac and in Linux sandboxes |
| Headless server and thin client | Yes. This is the biggest win |
| SSH transport | No. Needs a relay |
| CRDTs | No |
| GPUI | Yes, chosen |

### GPUI

- GPUI is Apache-2.0, and so are `gpui_ios`, `gpui_web`, `gpui_macos` and `gpui_wgpu`.
- Zed's `ui`, `editor`, `terminal`, `terminal_view`, `git_ui`, `remote`, `remote_server`, `proto`, `collab`, `acp_thread`, `agent_ui`, `workspace` and `project` crates are **GPL-3.0**.
- **iOS:** `gpui_ios` landed on 2026-10-14 as experimental, "not full desktop or mobile feature parity". It is published as `gpui-pre-ios` 0.3.11 (Apache-2.0), the same GPUI version gpui-kit 0.9.0 pins. It owns the app and scene delegates, and we found no API for embedding a GPUI view in a native screen, though an open PR adds "native window composition support". Its text input bridge covers marked text, autocorrect, spell check and return key types. The initial PR said the Return key and hardware shortcuts weren't delivered as GPUI key events, and file pickers are not implemented. Zed has no Android crate, and we found no sign of one in progress.
- **Web:** `gpui_web` landed on 2026-02-26 and is active. It uses WebGPU with a WebGL2 fallback. It paints to one canvas, ships no system fonts, and the multithreaded build needs nightly Rust and cross-origin isolation headers. gpui-kit labels it `showcase-only`.
- GPUI has AccessKit integration.

### gpui-kit (0.9.0, by Longbridge)

- The license header is Apache-2.0, and it's used in the shipping Longbridge Pro app.
- It has 75+ components, including `message`, `message_scroller`, `diff`, `attachment`, `dock`, `sidebar`, `command`, `setting` and `native_menu`.
- It also has variable-height virtual lists, a code editor, Markdown and HTML rendering, headless UI tests, and an experimental `gpui-webview` crate.
- **No terminal was found,** so we would build on `alacritty_terminal`.
- **API churn:** it pins exact `gpui-pre` snapshots because "any snapshot may change GPUI's API".
- **Mobile:** experimental and not a primary target. It uses `gpui-pre-mobile`, a fork of the community `gpui-mobile`. Both are licensed `GPL-3.0-or-later OR AGPL-3.0-or-later OR Apache-2.0`, so Apache-2.0 can be elected, subject to checking the dependency tree. GitHub's API labels the repos AGPL-3.0, which is misleading. The fork's main branch is on GPUI 0.3.10 while the kit is on 0.3.11, and mixing versions produces incompatible types. Its docs report that an AI chat area with text selection "passed functional and performance testing" on iOS and Android, and that native UI can share a screen with GPUI. The fork also covers Android, including TalkBack. Not yet verified by us.

## Sources

- Zed: [repo](https://github.com/zed-industries/zed), [iOS backend](https://github.com/zed-industries/zed/pull/63068), [web backend](https://github.com/zed-industries/zed/pull/50228), [remote development](https://zed.dev/docs/remote-development)
- gpui-kit: [repo](https://github.com/longbridge/gpui-kit), [WebAssembly](https://github.com/longbridge/gpui-kit/blob/main/website/docs/webassembly.md), [mobile](https://github.com/longbridge/gpui-kit/blob/main/website/docs/mobile.md)
- Alternatives: [Orca](https://github.com/stablyai/orca), [Paradev](https://github.com/ParadevAI/Paradev), [Paseo](https://github.com/getpaseo/paseo), [Emdash](https://github.com/generalaction/emdash)
