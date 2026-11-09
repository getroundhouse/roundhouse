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

**A prompt that carried a screenshot and a log, and two more files waiting in the composer.**

![Attachments in the transcript and the composer](screens/attachments.png)

**The screenshot, opened.**

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
- **Hovering an edit shows its diff,** so a change can be checked without opening it. The preview is a dozen lines tall at most. *Unconfirmed.*
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
- **Chat keys are the ones tabs use elsewhere:** `Cmd-T` for a new chat, `Cmd-W` to close one, and `Ctrl-Tab` and `Ctrl-Shift-Tab` to move between them. *Unconfirmed.*
