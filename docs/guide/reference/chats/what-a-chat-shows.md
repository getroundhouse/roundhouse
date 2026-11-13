# What a chat shows

A chat shows your prompts, what the agent wrote, and everything it did on the way. Every tool call is one line that opens to its detail, so a long turn stays readable.

## While the agent works

- **The working line ends the chat:** the mark with its light sweeping, and how long the turn has taken so far.
- **A thought still being written** is a muted line above it.
- **A prompt you've sent that the agent hasn't read** shows faint, above the working line.

![The agent partway through writing a reply](../../../spec/screens/streaming.png)

## A finished turn

A finished turn ends with how long it took and when, such as `1m, 48s · 2:15 PM`, and a button that copies the agent's final message.

Rest the pointer on that line for the turn's card: which model answered, when the turn started and ended, and how many tokens its requests took, read, written and cached.

![A finished turn's card](../../../spec/screens/turn-card.png)

## Tool calls

Each call is a line: what it is, and what it's about. Click it to open it.

| Call | Its line says | Opened, it shows |
|---|---|---|
| A command | What the agent said it's doing, and the command | The command, then what it printed |
| Reading a file | The file, and how much was read, such as `Read 10 lines` | The lines, numbered and highlighted |
| An edit | The file, and lines added and removed, such as `+5 −0` | Its diff |
| A search | Its pattern, where it looked, and how much it found | The matching lines, under their files |
| A web search | What it looked for, and how many pages it found | The pages, which open in the browser |
| A fetch | The site | The address, what was asked of it, and the answer |
| Any other tool | Its name made readable, and the MCP server it's from | Its input, then what came back |

![Commands, one opened with its output](../../../spec/screens/tool-commands.png)

- **A call that failed says so on a red line under it,** such as `Exit 1`, with the first line of what went wrong. It shows while the call is closed, so a failure is never hidden.
- **Hovering an edit shows its diff,** without opening it.
- **A command is text to select,** and a button copies all of it.
- **An image a call sent back, such as one it read, opens with a click,** as big as it is, up to what fits in the window. Escape or a click closes it.

![File calls, an edit opened to its diff](../../../spec/screens/tool-files.png)

## Thinking

What the agent thought is a line like a call's, saying `Thinking`, with the start of the thought. Click it to read the whole thought.

`Always expand thinking`, in [Settings](../settings.md), shows every thought in full.

## Subagents

An agent can hand part of its work to a subagent. Its call says what the subagent is for, its kind, the model it runs on, and how many calls it's made, such as `Haiku 5.5 · 3 calls`.

- **Opened, the call shows what the subagent was asked,** what it's done lately while it works, and its report once it's done.
- **`Open`, at the end of the line, opens the subagent tab,** with everything the subagent did, in order.
- **The band at the top of the tab says where the subagent stands,** such as `Working · Opus 5.5 · 3 calls · 24.6k context`. Its context is how many tokens its conversation holds, not what it has used in all, so it can fall when the subagent compacts.
- **A workspace has one subagent tab.** Opening another subagent shows that one in the same tab. `Cmd-W` closes it.

![Two subagents' calls, opened](../../../spec/screens/tool-subagent.png)

A subagent's work is kept out of the chat, so it doesn't bury the conversation it's part of.

## Work in the background

A turn can end with work still going: a command, a subagent, or a monitor watching for something. Up to four tasks have a line each where the chat ends, saying what each is for.

![A turn that ended with the test suite running in the background](../../../spec/screens/background.png)

With more than four, each kind with several has one line, such as `6 subagents`, so the lines never take the conversation's place. Click a kind's line to open it to its tasks' own lines.

![Thirteen tasks in the background, the subagents' line opened](../../../spec/screens/background-many-open.png)

- **The mark shows something is running:** a slower light on its inner row, or the light still sweeping the whole fan while a subagent is going.
- **The agent picks up again by itself** when the work finishes.
- **A wait on the pull request has a line of its own,** such as `Waiting on #182's checks · 2 checks running`, with `Stop` to call it off.
- **Closing the chat asks first,** while something is running.

## Images in a reply

An image the agent's reply points to shows in the reply, if it's in the workspace. The hub keeps it as it was when the reply came, so it's still there after the agent changes the file, and after the workspace is archived.

An image anywhere else on the machine, or on the web, shows nothing.

![A reply that points to a screenshot in the workspace](../../../spec/screens/reply-image.png)

- **A click opens it,** as big as it is, up to what fits in the window, whether it's on a line of its own or among the reply's words. Escape or a click closes it.
- **An image in a link goes where the link does** instead, as a badge does.
- **Some images open nothing:** one in a heading, a table or a footnote, one written with its address given elsewhere in the reply, as `![The board][board]` is, several together in bold or italics with no words, one beside nothing but HTML, and one in what a subagent did, shown under its call.

![The board's screenshot, opened](../../../spec/screens/reply-image-opened.png)
