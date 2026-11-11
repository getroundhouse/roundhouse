# Chats

## A turn

- **A turn can end with work still going in the background,** such as a test run the agent said it would come back to. Until that work finishes, an update waits for it and closing its chat asks first, and its mark shows something is running, as `look.md` says. The composer still sends rather than offering to stop, since stopping a turn doesn't reach that work, and when it finishes the agent picks up again by itself. *Unconfirmed.*
- **The Mac stays awake while there's work,** so it doesn't go to sleep by itself partway through. Work is a turn, what an agent left running in the background, and a workspace being set up. An agent that's only waiting on you isn't, since it could wait all night. The Mac is let go half a minute after the last of the work, so it stays awake between one turn and a queued prompt's. The display still sleeps, and closing the lid or a low battery still puts the Mac to sleep. `Keep computer awake while agents are working`, in Settings, turns it off.
- **What the agent's CLI writes only for the model isn't shown,** such as how much it shrank an image it read, or the summary a compacted conversation carries on from. *Unconfirmed.*
- **What's running in the background shows where the conversation ends,** a line for each task: a ring, and what the agent said it's for. The ring says it's in the background, so the line's words don't, and pointing at the line does. The fan stays the agent's, one to a chat, so a line under the working line isn't taken for another agent at work. A line goes when its task does. The hub keeps the list, so it shows however long ago the task started. *Unconfirmed.*
- **A ring's light goes round while its task runs, and a monitor's ring holds still with its hub lit,** since a monitor does nothing until what it's watching for happens. A command and a subagent both run. *Unconfirmed.*
- **Every task has a line, however many there are,** in the order the agent started them, since what each is for is the reason to look. *Unconfirmed.*
- **While the agent works, the chat ends with the working line:** its mark with the light sweeping, and how long the turn has taken so far, to a tenth of a second, so the count moving says it's still at it. The time is in the code font, so its digits stay put as they count. It counts from when the turn started, by the hub's clock, so every client counts the same. When the agent picks up again by itself after work in the background, or takes a prompt sent as its last turn ended, that's a new turn, and the count starts again.
- **The working line is above the lines for work in the background,** so when the turn ends, its end takes the working line's place and they stay where they are.
- **The working line goes while the agent waits on you,** since the card says so, and when it comes back its time counts the wait too.
- **A thought the agent is still writing is a muted line above the working line, with the icon its line will have once it's recorded and no mark of its own,** since the working line under it has one. A call it's still writing doesn't show until it's recorded, since it isn't running yet, and the working line says the agent is at work.
- **A finished turn ends with how long it took and when it ended,** such as "1m, 48s · 2:15 PM", and a button that copies the agent's final message. It says only as much of the day as it takes to tell when: none today, the day of the week within the last week, and the date before that. A stopped turn says "Stopped" instead, and one that failed says why, in red.
- **How long a turn took is counted the way the working line counts it,** by the hub's clock from the turn's start to its end, so the time doesn't jump when the turn ends, and doesn't change later. A turn from a hub too old to say when it started shows the agent's own count instead.

**A finished turn,** with tools, a list and code, and a pull request open for the branch. *Unconfirmed.*

![A finished turn](screens/conversation.png)

**A turn that ended with the test suite running in the background, and a monitor watching it for failures.** *Unconfirmed.*

![Work in the background](screens/background.png)

**The agent still working, with the test suite it started and a monitor watching it in the background.** *Unconfirmed.*

![Working with work in the background](screens/working-background.png)

**The agent partway through writing a reply, and how long it's been working.** *Unconfirmed.*

![A reply being written](screens/streaming.png)

- **An image a reply points to shows in the reply, if it's in the workspace,** written as Markdown, such as `![The board after](docs/screens/board-after.png)`, by its path in full or from the workspace. One written into the reply as a `data:` address shows too. On a line of its own, it's as wide as it is, up to the column's, and among words it's the height of the text. It's the file as it was when the reply came, which the hub keeps, so when the agent changes the file and points to it again, each reply keeps what it showed, and every client shows the same, even after the workspace is archived. One that wasn't there when the reply came shows nothing, and so does one in a reply from before the hub kept them. One anywhere else on the machine or on the web shows nothing, since what the agent writes can't be trusted to point only where it should, and neither does anything in the workspace that isn't an image. *Unconfirmed.*

**A reply that points to a screenshot in the workspace.** *Unconfirmed.*

![A reply with an image in it](screens/reply-image.png)

## Finding a prompt

- **A chat with more than one prompt has an outline at its right: a short tick for each prompt, top to bottom in the order they were sent, with the one being read in white,** so a long chat shows how far it goes and where you are in it. The one being read is the last to have reached the top of the chat, and at the end of the chat it's the last prompt, since nothing after it can reach the top. A chat with one prompt has nowhere to jump to, so it has no outline. *Unconfirmed.*
- **The outline shows sixteen ticks at most.** A longer chat shows those around the prompt being read, and a tick at an end with more past it is fainter. *Unconfirmed.*
- **The ticks sit at the top, in the margin between the scrollbar and the column,** narrow enough that in a narrow pane they stay clear of a prompt at the right of the column. They're a place to rest the pointer, not to click, so they can be that small. The wheel over them scrolls the chat, and doesn't open the list however long it turns, since a trackpad's wheel leaves the pointer resting there without asking for it. The list opens once the pointer moves over them. *Unconfirmed.*
- **Resting the pointer on the ticks lists the prompts over them, numbered, with up to two lines of each.** The one being read is lit and the rest are quieter, so it stands out among them. The list hides the ticks while it's open, and stays while the pointer is on it. It opens on the one being read, with the two before it above it, and it's as tall as its rows up to about eight prompts of two lines, past which it scrolls. The wheel over it scrolls only the list. A prompt that only carried files says how many. *Unconfirmed.*
- **In a pane narrower than the list, the list keeps its width,** running past the pane over what's beside it rather than squeezing its rows.
- **Clicking a prompt in the list puts that prompt at the top of the chat,** and the list closes, so it isn't over where it jumped to. It opens again once the pointer moves over the ticks, not because the click left it resting there. A prompt with less than a screen of chat below it can't reach the top, so the chat stays at its end, where that prompt shows already. *Unconfirmed.*
- **The outline has every prompt, however long the chat,** including those too far back for the app to have loaded, because the hub lists them. A hub too old to list them leaves only the prompts the app has loaded, and so does one that fails to, until the app reconnects to it, with the chat going on either way. A prompt shows in it once it's recorded. *Unconfirmed.*

**A long chat, jumped to its fourteenth prompt from the list.** *Unconfirmed.*

![A long chat and its outline](screens/outline.png)

**The outline's list, open over its ticks.** *Unconfirmed.*

![The outline's list](screens/outline-list.png)

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
- **In the transcript, a prompt's images show small above its text, and a click opens one to its own width, up to the prompt's.** Other files show their name and size. The hub keeps each image, so every client shows it, and it's still there after the workspace is archived. Other files are only in the workspace, so archiving takes them with it.

**A prompt that carried a screenshot and a log, and two more files waiting in the composer.** *Unconfirmed.*

![Attachments in the transcript and the composer](screens/attachments.png)

**The screenshot, opened.** *Unconfirmed.*

![An attached image, opened](screens/attachment-opened.png)

## Tool calls

Every call is drawn the same way, as `look.md` says, and each kind fills in what its line says and what it opens to. A tool that has no drawing of its own still gets a readable line, so a new tool, or one from an MCP server, never shows as raw JSON until it's opened.

- **A command's title is what the agent said it's doing,** with the command beside it, cut off with an ellipsis when it doesn't fit. A command the agent didn't describe says `Run`. Opened, the command comes first, highlighted as shell after a faint `$`, then what it printed. *Unconfirmed.*
- **A call that failed says so on a red line under it,** with how a command exited, such as `Exit 1`, or `Error` for anything else, and the first line of what went wrong. It shows while the call is closed, so a failure is never hidden. Opened, the call shows all of what went wrong, since the line only has room for its start, then what it tried, such as an edit's diff. The output itself stays in muted text, so it's still easy to read. *Unconfirmed.*
- **A result whose call is too far back to find is drawn on a line of its own,** saying `Output` and how many lines or images came back, and opens to them, so nothing the agent was told is lost. *Unconfirmed.*
- **Any other tool's title is its name made readable,** such as `Create issue` for `create_issue`, then the MCP server it's from in an outlined chip, and its most telling input in faint monospace. If the agent described the call, the description is the title and the tool's name follows it. Opened, its input as JSON, then what came back. *Unconfirmed.*

**Commands: one opened, one that failed, one the agent didn't describe, and one too long for its line.** *Unconfirmed.*

![Commands, one opened with its output](screens/tool-commands.png)

**Tools with no drawing of their own: one from an MCP server opened, one that failed, and a built-in one.** *Unconfirmed.*

![Other tools, one opened to its input and output](screens/tool-any.png)

- **A file call names the file in an outlined chip, and says how much it read or wrote,** such as `Read 10 lines` or `Write 12 lines`. Opened, the file's path, then its lines with their numbers in the file, highlighted for its language. The full path is only there, since the file's name is enough to tell calls apart. *Unconfirmed.*
- **A read of an image says `Read 1 image`, and opens to the image under the file's path, at its own width, up to the column's.** Any other tool's images, such as a screenshot from an MCP server, show the same way, after what else it sent back. The hub keeps them, as it does a prompt's, so every client shows them, and they're still there after the workspace is archived. *Unconfirmed.*
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

**A read of a screenshot, opened.** *Unconfirmed.*

![A read of an image, opened](screens/tool-read-image.png)

**Searches: one opened to its matching lines, one listing files, and one by name.** *Unconfirmed.*

![Searches, one opened to its matches](screens/tool-search.png)

**An edit and a search that failed, opened to all of what went wrong.** *Unconfirmed.*

![An edit and a search that failed, opened](screens/tool-failures.png)

- **A call that hands work to a subagent is titled with what the subagent is for,** with its kind of agent in an outlined chip, such as `Explore`, and how many calls it's made so far. *Unconfirmed.*
- **Opened, a subagent's call shows what the subagent was asked, then its latest call while it works, and its report once it's done,** in full, since the report is what the agent read and acted on. The rest of its work is in the subagent tab, which the call opens. *Not built:* for now, the call opens to everything the subagent did, in order, under a rule down its left: what it wrote, and its calls, each of which opens as any call does, and its last words are its report.
- **A subagent's work shows in the subagent tab, not in the chat.** The agent goes on working while a subagent does, several subagents can work at once, and one can make hundreds of calls, so their events arrive mixed with the agent's own. Kept out of the chat, they don't bury the conversation they're part of, and the chat costs the same to draw however much its subagents do. *Not built:* for now, a subagent's work is gathered into its call, and work that arrives too long after its call to be gathered, such as more than 2,000 events later, shows where it arrives instead, so it isn't lost.
- **The subagent tab works as the file tab does.** A workspace has at most one, and opening another subagent shows that one in the same tab, so following subagents doesn't leave a trail of tabs. It covers the chat and its composer until a chat's tab is clicked, and `Cmd-W` closes it while it shows. It sits just after the tab of the chat whose subagent it shows. It shows what the subagent was asked, then everything it did, in order, as the chat shows the agent's, then its report, and a subagent it started opens in the same tab. *Not built.*
- **A web search shows what it looked for and how many pages it found.** Opened, the pages, each its title and then its site, faint, which open in the browser when clicked, then what was made of them. *Unconfirmed.*
- **A fetch shows the site in an outlined chip.** Opened, the page's address, what the agent asked of it, then the answer. *Unconfirmed.*

**Subagents: one opened to its work, which came in among the agent's own, and one closed.** *Unconfirmed.*

![A subagent's call, opened to its work](screens/tool-subagent.png)

**The web: a search and a fetch opened, and a fetch the site refused.** *Unconfirmed.*

![A web search and a fetch, opened](screens/tool-web.png)

- **Asking the advisor says `Ask the advisor`.** The advisor is a stronger model that reads the whole conversation and advises the agent. Opened, what it said, whose start shows closed too, faint, as a thought's does. A reply can come encrypted, for the model alone, and then the call opens to a line saying only the agent can read it, and nothing claims to say what the advice was. When asking failed, the red line under the call says why, such as `overloaded`, or `refusal` when the advisor refused. *Unconfirmed.*
- **An open call to the advisor always shows something.** With no reply so far, it opens to a line saying the advisor hasn't replied, whether one is on its way or the turn ended first, since a reply can take minutes. A reply with nothing in it opens to a line saying it was empty. *Unconfirmed.*
- **A call to the advisor that the agent went on without says `No reply`.** The log says when the agent's next message began without a call's result. Its line says `No reply`, faint, so closed it reads apart from the call the agent then makes again, and it opens to a line saying the advisor didn't reply and the agent went on without it. It stays in the chat, since the agent did make it. This has been seen for a call made after a call to one of the agent's own tools, in the same message. *Unconfirmed.*

**The advisor: opened to a reply only the agent can read, then a call the agent went on without, the call it made again, closed on a reply that can be read, and a call that failed.** *Unconfirmed.*

![Asking the advisor, opened](screens/tool-advisor.png)

- **What the agent thought is a line like a call's,** saying `Thinking`, with the start of the thought, faint, cut off where the line ends. Opened, the whole thought is at body size in muted text, under its line and on the column's left edge, and the line drops the start, so it isn't shown twice. While it streams, its line says `Thinking…` at the same size. *Unconfirmed.*

**Thinking among calls: a long thought opened, and a short one closed.** *Unconfirmed.*

![Thinking, one opened in full and one closed](screens/thinking.png)

**A thought still being written, above the working line.** *Unconfirmed.*

![A thought being written](screens/thinking-streaming.png)

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

- **A new chat keeps the model, its effort and how edits are handled, but not Plan or Full access.** Plan belongs to the task at hand, and Full access should be chosen on purpose each time. *Unconfirmed.*
- **Every chat names its model and its effort, and never leaves either to the agent's CLI,** so the chat decides which model and level the agent runs, rather than the user's own Claude settings or the account. An alias such as `opus` is still the CLI's to resolve to a model. There's no default model to choose.
- **The model picker offers aliases:** `fable`, `opus`, `sonnet` and `haiku`. They follow the CLI's newest of each without a release here. *Unconfirmed.*
- **The model and its effort are one picker,** since how hard the agent thinks is part of choosing what thinks. It shows the model's name, then the effort in muted text, such as `Opus Extra high`, and its menu lists the models, then the levels: Low, Medium, High, Extra high and Max, which are the CLI's own. *Unconfirmed.*
- **A chat nobody has chosen for runs Opus at Extra high,** the owner's usual choice. *Unconfirmed.*
- **A new level reaches an agent that's already working without stopping it,** and shows once the agent has said it's using it, so the picker is never ahead of the agent. *Unconfirmed.*

**The approval mode's menu, open.** *Unconfirmed.*

![The approval mode's menu](screens/mode-picker.png)

**The model picker's menu, open, in a chat at Extra high.** *Unconfirmed.*

![The model picker's menu](screens/model-picker.png)

## The context and the account

- **A ring beside Send shows how full the agent's context is,** filled clockwise from the top. It shows once the agent has said, so a new chat has none until its first turn starts. It follows the agent as it works: the agent says when a turn starts, a few times a minute during one, and when it ends. It's in muted text's colour however full it is, since amber and red already mean something, and it doesn't move by itself, since only a working agent moves anything. *Unconfirmed.*
- **Resting the pointer on the ring shows a card above it,** with its right edge on the edge of the ring's place in the bar, so it covers neither the ring nor Send. It goes as soon as the pointer does. *Unconfirmed.*
- **The card says how much room is left.** First how much is used, as a share such as `63% used` and as a bar. Under the bar, how much more the context can take, such as `153.5k until it compacts`, and what's used of how much, such as `313.5k/500.0k`. An agent that keeps room back for shrinking the conversation has that room split off the bar's end by a gap, so the gap is where it compacts. For one that keeps none back, the bar is whole and the line says how much is left, such as `186.5k left`. *Unconfirmed.*
- **Then what's in the context, in the agent's own words, largest first, in tokens.** Room that's free or kept back isn't listed, since the bar shows it. Neither are the tools an agent loads only when it needs them, since they aren't in the context. A part whose kind the agent didn't say is listed, since it may be in it. *Unconfirmed.*
- **A bar for each of the account's limits sits left of the ring,** lying flat, one above the next and as wide as the ring, shortest window first. Each empties from the right as the limit is used, as its bar in the card does. They lie flat because two upright bars beside Send would read as a pause button. They're bars so they can't be taken for the ring, and in its colour for the same reason it is. They have a card of their own because a limit is the account's and the context is the chat's. They follow the account's record, which the hub changes as soon as any agent under the account hears of its limits, in whichever workspace, so they move with nothing asked for, while the app is in the background too. An agent says what's left only as it starts, so while one works it's asked again at most every five minutes, and the bars are never more than about five minutes behind what working agents have used. *Unconfirmed.*
- **Their card says how much of each limit is left,** such as `96% left` of the `7d limit`, as a bar that empties, and when the limit starts again, with the plan beside its heading. A limit is the account's and not the chat's, so the hub keeps what any agent under the account last heard, and every chat under that account says the same, in whichever workspace, and still says it after the hub restarts. A chat under another account has that account's own, even with the same model provider. Nothing shows for a limit that has started again since it was heard, for an account with no limits, or until an agent under the account has heard of one. *Unconfirmed.*
- **Under the limits, whose account it is:** the email, then more quietly how it's logged in and the model provider. The hub reads it from the agent's CLI on the workspace's machine each time an agent launches, and the chat names the account its agent's newest launch runs under, so it's there once the agent has gone. A line too long for the card, such as a long email, is cut off with an ellipsis. *Unconfirmed.*
- **With no limits to show there are no bars, so the ring's card says whose account it is,** under what the context holds: the email, then more quietly the plan, how it's logged in and the model provider. Until the chat's agent has launched, when nobody was logged in, and when the hub can't tell whose account it is, nothing says, and the ring's card is only the context. *Unconfirmed.*
- **How full a context is belongs to its chat's record,** so every client shows the same ring, and it's still there once the agent has gone or the hub has restarted, since the conversation the agent resumes is as full. It isn't in the chat's history: only the newest matters, and it answers the app's own question rather than being something the agent said. *Unconfirmed.*

**The ring's card, over a chat whose context is nearly two thirds full.** *Unconfirmed.*

![The context's card, open above its ring](screens/context.png)

**The limits' card, on an account with two limits.** *Unconfirmed.*

![The limits' card, open above their bars](screens/usage.png)

**The ring's card on an account no agent has heard a limit for, so it says whose account it is.** *Unconfirmed.*

![The context's card, with whose account it is under the context](screens/context-account.png)

## Tabs

- **The chat showing is underlined, not filled.** A 2px pale copper line runs under its tab, over the bar's bottom edge, and its title looks as it does when it isn't showing, as `look.md` says for lists. A fill would make the tab one more raised thing on the pane.
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
