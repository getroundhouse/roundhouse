# The diff

The sidebar of changes, the file tab, and comments for the agent.

## The sidebar of changes

The sidebar on the right, under the pull request's bar. It's open from the start, and `Cmd-Shift-B`, or the button at the right end of the header, hides it and shows it again.

Two tabs switch what it lists:

| Tab | Lists |
|---|---|
| **All files** | Every file in the worktree, ignored ones too |
| **Changes** | Every file the workspace changes against its base, with the lines each adds and removes |

- **A workspace that changes nothing shows All files,** and switches to Changes when it first changes something. A tab you click stays.
- **Changes shows as a list of paths, or in folders,** switched by the button at the sidebar's top right.
- **Each changed file has an icon for how it changed:** added, modified, removed or renamed.
- **Dragging the sidebar's inner edge changes its width,** from 300px to 800px.

![The same changes in folders](../../spec/screens/changes-tree.png)

## All files

- **Files show in folders, which start closed.**
- **A folder git ignores whole,** such as `target` or `node_modules`, is read only when it's opened.
- **Each file has a coloured icon for its kind,** as editors show them.
- **The list is read again when a turn ends,** so what the agent makes shows up without asking.

![All files](../../spec/screens/all-files.png)

## The file tab

Clicking a file shows it in the workspace's file tab: its diff from Changes, or the file as it is from All files.

- **A workspace has one file tab, always the leftmost.** Clicking another file shows that one in the same tab.
- **It covers the chat until a chat's tab is clicked,** and stays open behind the chats until it's closed.
- **`Cmd-W` closes it while it shows,** and `Ctrl-Tab` comes to it after the last chat.
- **The bar at its top has the file's path,** and a button after it that copies the path while the pointer is on the bar.

## A file's diff

- **Changes show with three lines either side,** and the lines between fold into a row that says how many there are.
- **Arrows on that row show 20 more lines at a time,** or all of them when there are no more than 20.
- **Lines added and removed are tinted green and red,** and the words that changed more strongly.
- **A switch at the right end of the bar** shows the diff in one column, side by side, or as the whole file.
- **A Markdown file's diff can show rendered,** the fourth choice on the switch: what the change removed is struck through on red, and what it added is on green, each where it was. A paragraph that changed shows as it was, then as it is.
- **What you've opened stays open** when the diff is read again, such as at the end of a turn.

![A file's diff side by side](../../spec/screens/diff-split.png)

![A Markdown file's diff rendered](../../spec/screens/diff-rendered.png)

## Comments

- **Hover a line and click its add button** to comment on it. `Enter` saves the comment.
- **To dictate, focus the comment and hold Space or `Option-Space`.** Finish dictation keeps the words in the box; `Enter` finishes them and saves. See [Dictate a prompt](../guides/dictate-a-prompt.md).
- **A saved comment shows where it was written,** with `Edit`, `Delete` and `Add to chat`.
- **A file with comments is marked in the sidebar,** with a comment's icon and how many, and so is a closed folder holding one.
- **`Add to chat` puts one comment in the composer,** after what's typed there.
- **The agent reads every comment with Roundhouse's tools** when you ask it to, each with its file and line.
- **A comment stays on the diff until it's deleted.**
- **A line with a comment always shows,** rather than folding away.
- **A comment being written is kept by the hub,** so it's there when the app opens again.

## Changed images

A changed image shows what it was and what it is. One bar above it chooses Side by side, Before, After or Difference.

- **Both sides are drawn at one scale,** so a pixel is in the same place in each.
- **Clicking the image, or Space, flips between Before and After.**
- **Difference shows After faded to grey, with each changed pixel tinted yellow,** and an outline around each place that changed. A strip says how much, such as `3,775 pixels in 7 places`.

![A changed image, side by side](../../spec/screens/image-side-by-side.png)

Any other file that isn't text says so, and takes no comments.

## A file opened from All files

- **Its lines are numbered and highlighted for its language.** A file over 1 MB stays plain.
- **A Markdown file shows rendered,** as the chat shows Markdown, and its text is selected as the chat's is, with `Shift` and an arrow too. The switch at the right end of the bar shows its source instead, and the choice holds for every Markdown file until the app quits.
- **Text is shown up to 4 MB, and an image up to 64 MB.**
- **A WebM video plays in the tab,** as one in a reply does, with what kind it is, its size and how long it is above it.
- **Any other video, such as an MP4 or a QuickTime movie, says what it is, with Open in system player,** which opens it in the app your system plays it with. When the workspace is on another machine, the file is copied to this one first.
- **A file shown is read again when a turn ends,** keeping where it's scrolled to.
- **A file that can't be shown says why:** it isn't text, an image or a video, it's too big, it's empty, or it's gone.
