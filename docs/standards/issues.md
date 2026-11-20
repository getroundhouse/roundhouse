# Issues

How an issue is written, labelled, scheduled and linked to others. `docs/standards/documentation.md` says what goes in an issue rather than a doc, and `docs/standards/writing.md` how any text is worded.

## The description

- **It's written for whoever builds it next,** so it holds what happens, why, what to build, and what's still open. How it was found, the evidence for each claim, and counts and dates go in a comment, such as which processes were found running and when, as `docs/standards/documentation.md` puts results there. A reader who wants the proof can find it, and one who's building doesn't have to read past it.
- **Once something is decided, rewrite the description around it,** and move the options that weren't chosen to the comment that records who decided and when. Paragraphs added below leave the reader to work out which still holds. A question is the exception, since its options are what it's about: the answer goes at the top, and the options stay.
- **Before filing, read it once as that reader, and cut whatever wouldn't change what they do.** One that runs past about 500 words is usually holding a plan, so split it into sub-issues, as "What a plan holds" below says.

## Each kind's outline

Its parts come in this order. Leave out a part with nothing in it.

**A bug:**

1. What goes wrong and when, as someone who hit it would recognize it, and whether that was seen in the app or predicted from the code.
2. How a person gets there, when it isn't obvious.
3. Why it happens, naming the code.
4. Ways to fix it, when you see any.
5. A decision the owner has to make first, as a question, when there is one.

For example, an issue that opens:

> If a chat's saved effort or model is no longer offered, changing just its approval mode fails, with an error about the effort or the model rather than the mode.
>
> **How a chat ends up like that.** An update stops offering the effort or model a chat was using.
>
> **Why the change fails.** `LiveSession::configure` (`crates/hub/src/session/live.rs`) checks all three of the chat's settings after the change, not just the one being changed. So the effort that was already there fails the check, and the mode change is refused, though the new mode is fine.

**A feature or a task:**

1. What's true once it's done, in one concrete case: what a person sees, or what a caller gets back.
2. What's there already that it builds on, naming the code.
3. What to build, a bold-led bullet for each part.
4. What's still to find out, and a decision the owner has to make first, as a question.
5. When it's done, as something a test or a person could check.

**A question:**

1. What's undecided, and what waits on it.
2. What's known, from the code, a test or a doc.
3. Two to four options, each with what it costs, the recommended one first.

**A risk:** what could go wrong, what would show it happening, and what would mitigate it.

**A plan:** what the work achieves, in a paragraph, then its parts as sub-issues or a checklist, in the order they go, with why when the order isn't obvious.

## Labels and fields

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
