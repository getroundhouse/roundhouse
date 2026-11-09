# Chats

## A turn

- **A turn can end with work still going in the background,** such as a test run the agent said it would come back to. Until that work finishes, the agent counts as working: its mark moves, an update waits for it, and closing its chat asks first. The composer still sends rather than offering to stop, since stopping a turn doesn't reach that work, and when it finishes the agent picks up again by itself. *Unconfirmed.*
- **What's running in the background shows where the conversation ends,** one working line for each task, with its mark moving and what the agent said it's for. A line goes when its task does. The hub keeps the list, so it shows however long ago the task started. *Unconfirmed.*

**A finished turn,** with tools, a list and code, and a pull request open for the branch. *Unconfirmed.*

![A finished turn](screens/conversation.png)

**A turn that ended with the test suite running in the background, and a watch on it for failures.** *Unconfirmed.*

![Work in the background](screens/background.png)

**The agent partway through writing a reply.** *Unconfirmed.*

![A reply being written](screens/streaming.png)

## Titles

- **A chat's title can be changed by hand,** as a workspace's can, in `workspaces.md`.
- **A chat's title is generated from its first prompt,** by the same rules as a workspace's.
- **A workspace's first chat gets the same title as the workspace,** from the same call, since they come from the same prompt. *Unconfirmed.*

## Attachments

- **A prompt can carry files.** Paste an image, or drop images and files on the composer, or choose them with its `+`, and each shows as a tile above what's typed, with a way to take it off. A prompt with files needs no text, since a screenshot can be the whole question.
- **Any kind of file can go, up to 100 MB, and at most 10 with a prompt.** The size is what the hub takes in one go.
- **Each file goes in the workspace's `.context/attachments/`, and the agent is told where, so it can open it then or later.** Git is told to ignore the folder, so the files never show as changes. Images up to 32 MB between them, and a text file up to 32 KB, also go in the prompt itself, so the agent sees them without opening anything. A longer text file is opened from where it is, so the agent can search it rather than read all of it.
- **A file is known by its bytes rather than its name,** so the agent is never sent an image labelled as something it isn't, and a pasted TIFF, which some apps copy, goes as a PNG.
- **Files stay with their chat until they're sent,** the way what's typed does, and a prompt that can't be sent comes back to its chat's composer, files and all. While the agent waits on a question, what's typed answers it and the files wait for the next prompt, since an answer can't carry them.
- **In the transcript, a prompt's images show small above its text, and a click opens one to its own width, up to the column's.** Other files show their name and size. The hub keeps each image, so every client shows it, and it's still there after the workspace is archived. Other files are only in the workspace, so archiving takes them with it.

**A prompt that carried a screenshot and a log, and two more files waiting in the composer.** *Unconfirmed.*

![Attachments in the transcript and the composer](screens/attachments.png)

**The screenshot, opened.** *Unconfirmed.*

![An attached image, opened](screens/attachment-opened.png)

## Tool calls

Every call is drawn the same way, as `look.md` says, and each kind fills in what its line says and what it opens to. A tool that has no drawing of its own still gets a readable line, so a new tool, or one from an MCP server, never shows as raw JSON until it's opened.

- **A command's title is what the agent said it's doing,** with the command beside it, cut off with an ellipsis when it doesn't fit. A command the agent didn't describe says `Run`. Opened, the command comes first, highlighted as shell after a faint `$`, then what it printed. *Unconfirmed.*
- **A call that failed says so on a red line under it,** with how a command exited, such as `Exit 1`, or `Error` for anything else, and the first line of what went wrong. It shows while the call is closed, so a failure is never hidden. Opened, the call shows all of what went wrong, since the line only has room for its start, then what it tried, such as an edit's diff. The output itself stays in muted text, so it's still easy to read. *Unconfirmed.*
- **A result whose call is too far back to find is drawn on a line of its own,** saying `Output` and how many lines came back, and opens to them, so nothing the agent was told is lost. *Unconfirmed.*
- **Any other tool's title is its name made readable,** such as `Create issue` for `create_issue`, then the MCP server it's from in an outlined chip, and its most telling input in faint monospace. If the agent described the call, the description is the title and the tool's name follows it. Opened, its input as JSON, then what came back. *Unconfirmed.*

**Commands: one opened, one that failed, one the agent didn't describe, and one too long for its line.** *Unconfirmed.*

![Commands, one opened with its output](screens/tool-commands.png)

**Tools with no drawing of their own: one from an MCP server opened, one that failed, and a built-in one.** *Unconfirmed.*

![Other tools, one opened to its input and output](screens/tool-any.png)

- **A file call names the file in an outlined chip, and says how much it read or wrote,** such as `Read 10 lines` or `Write 12 lines`. Opened, the file's path, then its lines with their numbers in the file, highlighted for its language. The full path is only there, since the file's name is enough to tell calls apart. *Unconfirmed.*
- **An edit shows how many lines it added and removed,** as `+5 −0` in green and red, and so does a write over a file that was already there. A write that made a new file shows no counts, since all of it is new, and opens to the whole file marked as added. *Unconfirmed.*
- **An edit's diff uses the file's own line numbers,** from the patch the agent sent back, with a gap mark between parts of the file that aren't next to each other. Added and removed lines are tinted, with a bar down their left, and where a line changed in place, the part that changed is picked out. Before the patch comes back, the diff is worked out from the edit itself, without numbers. *Unconfirmed.*
- **Hovering an edit shows its diff,** so a change can be checked without opening it. The preview is a dozen lines tall at most, and stays while the pointer is on it, so a longer diff can be scrolled there. The wheel over it scrolls only the diff, so the transcript doesn't carry the edit away from it.
- **A search shows its pattern, where it looked, and how much it found,** such as `4 matches in 3 files`, or `Nothing found`. Opened, matching lines under their file's path, with their numbers, or the files it found. Anything else it sends back, such as matches with lines of context around them, shows as it came, so nothing is lost. Finding files by name says `Find files`. *Unconfirmed.*
- **A block of a file's lines is twelve and a half lines tall at most,** so a dozen fit whole and anything longer shows half a line as a sign that it scrolls. *Unconfirmed.*

**Each kind of file call: a read, an edit opened to its diff, a new file, an edit that failed, and a write over a file that was there.** *Unconfirmed.*

![File calls, an edit opened to its diff](screens/tool-files.png)

**An edit's diff, shown while it's hovered.** *Unconfirmed.*

![An edit's diff in a preview](screens/tool-edit-preview.png)

**A read of part of a file and a new file, both opened to their lines.** *Unconfirmed.*

![A read and a new file, opened](screens/tool-file-contents.png)

**Searches: one opened to its matching lines, one listing files, and one by name.** *Unconfirmed.*

![Searches, one opened to its matches](screens/tool-search.png)

**An edit and a search that failed, opened to all of what went wrong.** *Unconfirmed.*

![An edit and a search that failed, opened](screens/tool-failures.png)

- **A call that hands work to a subagent is titled with what the subagent is for,** with its kind of agent in an outlined chip, such as `Explore`, and how many calls it's made so far. Opened, what it was asked, then everything it did, in order, under a rule down its left: what it wrote, and its calls, each of which opens as any call does. Its last words are its report. *Unconfirmed.*
- **A subagent's work shows only in its call.** The agent goes on working while a subagent does, and several subagents can work at once, so their events arrive mixed together, and drawn where they arrive they'd read as one story. Work that arrives too long after its call for the call to gather it, such as more than 2,000 events later, shows where it arrives instead, so it isn't lost. *Unconfirmed.*
- **A web search shows what it looked for and how many pages it found.** Opened, the pages, each its title and then its site, faint, which open in the browser when clicked, then what was made of them. *Unconfirmed.*
- **A fetch shows the site in an outlined chip.** Opened, the page's address, what the agent asked of it, then the answer. *Unconfirmed.*

**Subagents: one opened to its work, which came in among the agent's own, and one closed.** *Unconfirmed.*

![A subagent's call, opened to its work](screens/tool-subagent.png)

**The web: a search and a fetch opened, and a fetch the site refused.** *Unconfirmed.*

![A web search and a fetch, opened](screens/tool-web.png)

- **What the agent thought is a line like a call's,** saying `Thinking`, with the start of the thought, faint, cut off where the line ends. Opened, the whole thought is at body size in muted text, under its line and on the column's left edge, and the line drops the start, so it isn't shown twice. While it streams, its line says `Thinking…` at the same size. *Unconfirmed.*

**Thinking among calls: a long thought opened, and a short one closed.** *Unconfirmed.*

![Thinking, one opened in full and one closed](screens/thinking.png)

## Sending while the agent works

- **Enter sends what's typed to the agent while it works, and the agent reads it at its next chance,** such as when a running tool finishes. Until then the prompt is faint, so it's clear the agent hasn't seen it. One sent while the agent writes its last reply starts a turn of its own straight after. *Unconfirmed.*
- **Stopping the agent drops a prompt it hasn't read yet, and the prompt says so,** since stopping means stopping. So does a prompt the agent never read because it went away. *Unconfirmed.*
- **Cmd-Enter queues what's typed instead, for after the agent finishes.** Each queued prompt is a turn of its own, sent in order once the agent has nothing left to do, so it can't fold into a turn it wasn't meant for. With the agent idle, Cmd-Enter sends. A queued prompt can't carry files yet, so Cmd-Enter leaves a prompt with files in the composer and says so. *Unconfirmed.*
- **The queue sits above the composer, one line to a prompt, with buttons to change one, take it out, or send it now.** A change is made in place, where Enter keeps it and Escape leaves it as it was. Once the agent has a prompt, it can't change. *Unconfirmed.*
- **A queued prompt's buttons show only while the pointer is over it,** which lights its row, so it's clear which prompt they're for.
- **The arrow on a queued prompt steers the agent with it,** as Enter does, rather than leaving it to wait its turn.
- **The queue belongs to the hub, not the app,** so it's still sent with the app closed, and every client shows the same queue. *Unconfirmed.*
- **Stopping the agent holds the queue, and so does the agent going away mid-turn,** since then it's for the person to say whether to go on. The queue says it's waiting, and the arrow on a prompt sends it as a turn of its own, after which the rest follow. Sending any prompt lets the queue go on, once that turn ends. The hold is on the session's record, so it lasts through a hub restart. *Unconfirmed.*

**A prompt sent while a tool runs, waiting for the agent.** *Unconfirmed.*

![A prompt waiting for the agent](screens/steered.png)

**Three prompts queued while the agent writes, with the pointer over one.** *Unconfirmed.*

![The queue](screens/queued.png)

**The queue held after a stop.** *Unconfirmed.*

![A held queue](screens/queue-held.png)

## Questions and approvals

- **While the agent waits on a question, what's typed in the composer is the answer,** and the card says so. A plain Allow button would throw the question away. *Unconfirmed.*

**A command waiting to be allowed.** *Unconfirmed.*

![A command waiting to be allowed](screens/approval.png)

**A question from the agent, with choices.** *Unconfirmed.*

![A question with choices](screens/question.png)

## The model and approval mode

- **A new chat keeps the model and how edits are handled, but not Plan or Full access.** Plan belongs to the task at hand, and Full access should be chosen on purpose each time. *Unconfirmed.*
- **The model picker offers aliases:** `default`, `fable`, `opus`, `sonnet` and `haiku`. They follow the CLI's newest of each without a release here. *Unconfirmed.*

**The approval mode's menu, open.** *Unconfirmed.*

![The approval mode's menu](screens/mode-picker.png)

## Tabs

- **The chat showing is underlined, not filled.** A 2px pale copper line runs under its tab, over the bar's bottom edge, and its title is in the foreground colour while the others' are muted. A fill would make the tab one more raised thing on the pane.
- **A tab's close button shows only while the pointer is over the tab,** the one showing included, so the row reads as a row of titles. `Cmd-W` closes the chat showing without it.
- **Every tab is 200px wide,** with its mark and as much of its title as fits, so the row doesn't shift when a chat gets its title. *Unconfirmed.*

**Three chats, with the pointer over one that isn't showing.** *Unconfirmed.*

![Chat tabs](screens/chat-tabs.png)

## Closing chats, and keys

- **Closing a chat stops its agent and keeps its history.** Closing the last one starts a new one rather than leaving nothing, and a workspace's first chat is found or made by the hub, so two clients opening a workspace at once share one chat. *Unconfirmed.*
- **A closed chat is reopened from the history button at the far end of the tabs,** which shows once a chat is closed and stays put as chats open and close. It's a muted icon with no fill, which brightens while the pointer is over it or its menu is open. The menu lists the closed chats, the most recently closed first, each with its title, how long ago it was closed, such as `9m ago`, and an icon saying a click reopens it. A row has no mark, because a mark says what a chat's agent is doing, and closing a chat stops its agent. *Unconfirmed.*
- **Chat keys are the ones tabs use elsewhere:** `Cmd-T` for a new chat, `Cmd-W` to close one, and `Ctrl-Tab` and `Ctrl-Shift-Tab` to move between them. *Unconfirmed.*

**Two closed chats, in the history button's menu.** *Unconfirmed.*

![Closed chats](screens/closed-chats.png)
