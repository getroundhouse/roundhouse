# Keys

The app's keyboard shortcuts. They're written as a Mac's. On Windows and Linux, `Ctrl` takes the place of `Cmd`, so `Cmd-N` is `Ctrl-N` there.

## Workspaces

| Key | Action |
|---|---|
| `Cmd-N` | New workspace |
| `Cmd-[`, `Cmd-]` | The workspace before, and after |
| `Cmd-Shift-A` | Archive the workspace |
| `Cmd-Shift-U` | Mark the workspace read, or unread again |
| `Cmd-Shift-D` | Show or hide the sidebar of changes |
| `Cmd-Shift-P` | Ask the agent for a pull request |

## Chats

| Key | Action |
|---|---|
| `Cmd-T` | New chat |
| `Cmd-Shift-T` | New terminal, in a tab after the chats |
| `Cmd-W` | Close the chat, or the file tab, subagent tab or terminal while it shows |
| `Ctrl-Tab`, `Ctrl-Shift-Tab` | The tab after, and before |
| `Cmd-F` | Find in the chat |
| `Cmd-G`, `Cmd-Shift-G` | The next match, and the one before |
| `F3`, `Shift-F3` | On Windows and Linux, the next match, and the one before, too |
| `Enter`, `Shift-Enter` | In Find's bar, the next match, and the one before |
| Escape | Close Find's bar, or an image opened from the chat |

## A terminal

Every key typed in a terminal goes to its program, as in a terminal app, but the app's keys on `Cmd` on a Mac and on `Ctrl-Shift` on Windows and Linux, and `Ctrl-Tab` on both.

| Key | Action |
|---|---|
| `Cmd-C` | Copy what's selected. `Ctrl-Shift-C` on Windows and Linux |
| `Cmd-V` | Paste. `Ctrl-Shift-V` on Windows and Linux |
| `Cmd-A` | Select everything, the history too. `Ctrl-Shift-A` on Windows and Linux |
| `Cmd-Shift-T` | Another terminal |
| `Cmd-Left`, `Cmd-Right` | On a Mac, the start of the line, and its end, in a shell |
| `Cmd-Backspace` | On a Mac, delete back to the start of the line in bash and fish, or the whole line in zsh |
| `Option-Left`, `Option-Right` | On a Mac, back a word, and on one, in a shell |

On Windows and Linux, `Ctrl` with a letter goes to the program, as `Ctrl-W` deletes a word in a shell, so in a terminal the app's own keys take `Shift` too: `Ctrl-Shift-W` closes the terminal, `Ctrl-Shift-N` makes a new workspace and `Ctrl-Shift-Q` quits. The app's other keys go to the program there, `F10` and `Ctrl-,` too, so New chat, Settings and the workspace before and after are in the menus, which the button in the corner opens. `Ctrl-Shift-C`, `Ctrl-Shift-V` and `Ctrl-Shift-A` copy, paste and select all there, so Archive is in the File menu.

Find and stopping the agent are for a chat, so they do nothing unless the chat shows, such as while a terminal, a file tab or a subagent tab does.

## The composer

| Key | Action |
|---|---|
| `Enter` | Send. While the agent works, steer it |
| `Cmd-Enter` | Queue a prompt for when the agent finishes |
| `Cmd-.` | Stop the agent's turn |
| `Shift-Tab` | Turn plan mode on or off |
| `Enter`, Escape | While changing a queued prompt, keep the change, or leave it as it was |

## A question from the agent

| Key | Action |
|---|---|
| A number | Choose that option |
| `0` | Type another answer |
| Left and right arrows | The question before, and after |
| `Enter` | Go to the next question without an answer, and send once there's none |
| Escape | Leave the row for typing an answer |

## The new-workspace dialog

| Key | Action |
|---|---|
| Return | Create |
| `Shift-Return` | A new line |
| `Shift-Tab` | Turn plan mode on or off |
| Escape | Close, keeping what's typed |

## Reviewing

| Key | Action |
|---|---|
| `Enter` | Save a comment |
| Space | Flip a changed image between Before and After |

## The app

| Key | Action |
|---|---|
| `Cmd-,` | Open Settings |
| Escape, `Cmd-W` | Go back from Settings |
| `Cmd-F` | In Settings, search them |
| `Cmd-H` | On a Mac, hide Roundhouse |
| `Cmd-Option-H` | On a Mac, hide the other apps |
| `Cmd-M` | On a Mac, minimize the window |
| `Cmd-Q` | Quit. Agents keep running |
| `F10` | On Windows and Linux, open the menus, or close them |

Each menu item shows its key beside it.
