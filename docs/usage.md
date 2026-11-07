# Measured usage

What parallel agent work actually looks like, measured from the user's own history in Conductor, as aggregate numbers only. They set the test sizes in `docs/performance.md`. The cloud numbers came from `.context/measure_cloud.py`, with its results in `.context/cloud_shape.json`, both gitignored and only in the `yaounde` workspace.

## Local data

| Metric | Value |
|---|---|
| Repos, workspaces, sessions | 17, 598 (574 archived), 911 |
| Sessions per workspace | About 1.5 |
| Messages | 434,787 |
| Messages per session | p50 275, p90 1,137, p99 3,164, max 5,974 |
| Largest session | 28.8 MB |
| Largest single message | 2.3 MB |
| Role mix | 98% assistant rows, 2% user |
| Diff comments | 513 |
| Agents | 591 Claude and 320 Codex sessions |

What this means:

- Workspaces are disposable, so creating and cleaning them up must be fast and reliable.
- Long transcripts are normal, so rendering must handle them efficiently from day one.
- Review is a core loop.

## Cloud data

`conductor.db` holds no cloud workspaces. `cache.db` is a partial cache (28,630 messages across 67 sessions, longest 3,466). Authoritative numbers come from the API, using aggregates only. The API's SQL endpoint returned "temporarily disabled" on 2026-11-06, so these were measured through REST.

| Metric | Value |
|---|---|
| Workspaces, sessions | 604, 981 |
| Sessions by event count | Under 1k: 784. 1k to 5k: 179. 5k to 10k: 16. 10k to 25k: 2 |
| Longest sessions | 14,205, 11,324, 9,336, 8,272 events |
| Longest session size | 51 MB, about 3.8 KB per event |
| Largest single event | 1.2 MB |

Shape of the longest session:

| Block | Count | p50 chars | p99 chars | Max chars |
|---|---|---|---|---|
| Tool use | 2,863 | 318 | 7,068 | 49,939 |
| Tool result | 2,863 | 488 | 23,269 | 389,461 |
| Assistant text | 1,450 | 575 | 10,216 | 24,226 |
| Thinking | 1,288 | 1,918 | 23,411 | 72,276 |
| User messages | 55 | 296 | 1,018 | 1,018 |

What this means:

- About 60% of events render as visible content; the rest are system and lifecycle events.
- Bash is 83% of tool uses and MCP tools are 12%, so tool cards and command output dominate the transcript.
- The longest cloud session is about 2.4x the longest local one, though cloud counts events and local counts message rows, so the units may differ. Sizes are heavy tailed rather than huge across the board: a real worst case is about 15k items with a few outliers over 1 MB.
