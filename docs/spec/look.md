# Look

The rules every screen follows: surfaces, type, space, buttons, colour, and the status mark. Each area's file shows its own screens. `crates/look` holds the theme, the colours the kit has no name for, and the mark.

It's meant to feel familiar to the people Roundhouse is for, and it sets itself apart with a motif of its own, the status mark.

## Surfaces and type

- **Three warm near-black surfaces,** so the app reads as one calm field with a few raised things on it. The pane is `#151312`, the sidebar `#1a1715`, and anything raised, such as a message, a card, a popover or the selected tab, is `#24201e`. The composer and code blocks are `#1e1b19`.
- **One thin edge colour, `#2a2624`, and no shadows.** Edges separate, and nothing floats.
- **The system font, at 14px for body text, 12px for secondary text and tool calls, and 11px for small details.** It's dense enough to watch many agents, and still easy to read.
- **Emphasis is medium weight, never bold.** Bold shouts in a dense layout.
- **Three text colours:** foreground `#ebe7e3`, muted `#a39d97`, and faint `#6f6964` for meta, line numbers and hints.
- **Code is Menlo at 12px.**

## Space and shape

- **A 4px grid.** Controls are 28px tall, sidebar rows 30px, the workspace header 48px, and the chat tabs 40px.
- **Corners are 6px on controls, and 10px on cards and the composer.**
- **The transcript is a centred column, at most 780px wide,** so lines stay short enough to read in a wide window.
- **A tool call is one compact line,** with an icon for its kind and the input that matters, and its output folds underneath it.

## Buttons

- **Each area has at most one main action,** with a light fill and dark text, such as Create PR, Send, Add repository or Send to agent. With one obvious next step per area, nothing competes with it.
- **Every other button is borderless until hovered.** A button with an outline opens something, like the pull request.

## Colour

Each colour means one thing.

- **Copper, `#d08a5c`, is the brand.** It's for empty states, the app icon and keyboard focus, and never a status.
- **Amber means it's your turn, and red means the agent stopped short.** A card that needs you, such as a question or an approval, gets a thin amber edge and a faint amber background.
- **A pull request's state is shown in GitHub's colours,** which the people Roundhouse is for already know: green when it's open, purple when it's merged, a muted red when it's closed, and a grey of its own when it's a draft.

## The status mark

Every workspace and chat has a mark: a half circle of fine dots around a turntable, like a roundhouse seen from above. It has four rows of dots, spaced evenly along each row, so outer rows have more of them and the edge stays as dense as the middle. Fine dots give it a texture of its own. It's 18px wide in the sidebar and 16px in chat tabs, cards and the working line.

The mark has three layers, and each is about one thing, so several facts can show at once without competing:

| Layer | It's about | What it shows |
|---|---|---|
| The fan's colour | The work | Where its pull request stands: grey for none, then green, purple, muted red or a lighter grey, as above |
| A white light over the fan | The agent | A light sweeps the fan while the agent works, and the fan fills in while the workspace is set up |
| The whole fan in amber or red | You | Amber when it's your turn to answer or approve, red when the agent stopped short. The pull request's colour comes back once that's dealt with |

- **Only a running agent moves the mark.** Motion says the agent is working, so nothing else moves. When it's your turn, the fan turns amber and holds still. *Not built:* the marks are drawn still, at one moment of their motion.
- **A workspace's mark speaks for its open chat that most needs attention.** `sidebar.md` has the rule.
- **The sidebar shows every workspace's pull request.** *Not built:* only a workspace that's open knows its pull request, until the hub sends it with each workspace. The others show no pull request.

## Empty states

- **An empty pane shows the mark in copper, what's missing and why, and the one thing to do about it,** as a main button.
