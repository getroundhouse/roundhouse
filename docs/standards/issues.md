# Issues

How an issue is labelled, scheduled and linked to others. `docs/standards/documentation.md` says what goes in an issue rather than a doc, and `docs/standards/writing.md` how one is worded.

Labels answer three questions, so any common search is one filter: what kind of issue it is, which part of the system it's in, and whether only the owner can move it.

**Kind: exactly one.** It says how the issue closes. An issue with no kind hasn't been triaged.

| Label | What it is | It closes when |
|---|---|---|
| `kind/bug` | Something built that behaves wrongly, including under failure | It's fixed, with a test |
| `kind/feature` | Something a person using the product would notice is missing | It's built |
| `kind/task` | Work no user sees: tooling, docs, setup, refactors, code brought up to a standard | It's done |
| `kind/question` | Something not settled, whether a design or a decision | It's answered, and the outcome is in a doc |
| `kind/risk` | Something that could go wrong, watched rather than worked on | It's mitigated or accepted |
| `kind/plan` | An issue that breaks work into parts, or orders other issues | What it holds is closed |

**Area: one or more.** Areas follow the parts in `docs/reference/layout.md`, such as `area/hub` and `area/transcript`, and small or unbuilt parts share one, such as `area/mobile` for the iOS app and the relay. Each area's description on GitHub says what it covers. Four cover what isn't a part: `area/install` for building, installing and updating the app, `area/dev` for developing Roundhouse itself, `area/docs`, and `area/project` for the license, scope and overall plans. Nothing else becomes an area.

**`needs/owner`:** only the owner can move it, because it needs their decision, their account or device, or their judgment by hand. Agents leave these alone. A PR that changes an owner doc, or a file that enforces a standard, gets it too, and waits for the owner's approval.

Everything else uses GitHub's own fields rather than a label:

- **When:** a milestone. An issue with no milestone is in the backlog, and there are no priority labels.
- **What a plan holds:** a plan that breaks one piece of work into parts uses sub-issues, which nest and show progress. A plan that orders or selects issues with their own reasons to exist uses a checklist, since an issue can be in several.
- **Blocked:** one issue waiting on another, without being part of it, is a "blocked by" link.
- **Duplicate, or won't do:** the reason it was closed.
