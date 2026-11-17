# Workspaces

## Creating and setting up

**Before there's a project.** *Unconfirmed.*

![No projects](screens/first-run.png)

- **New Workspace in the File menu, `Cmd-N`, Create in the sidebar and on Home, and the `+` beside a project open a dialog that asks what to work on,** so the task is given as the workspace is made, and the agent starts on it at once. `Cmd-N` and Create open it for the project `sidebar.md` says, and a `+` for its own. *Unconfirmed.*
- **The dialog holds the project, the agent, the first prompt, the model and its effort, plan mode, and files to go with the prompt.** The project's name at the top opens a menu of the others. Files are pasted or dropped, as in a chat's composer. *Unconfirmed.*

  **The new-workspace dialog, opened over a workspace.** *Unconfirmed.*

  ![The new-workspace dialog with one offered agent](screens/new-workspace.png)

  **With more than one agent, choose the first chat’s agent.** *Unconfirmed.*

  ![Choose the first chat’s agent](screens/new-workspace-agents.png)

  **A task described in the dialog, with a screenshot to go with it, for Opus at extra high effort.** *Unconfirmed.*

  ![A task described in the new-workspace dialog](screens/new-workspace-written.png)

  **The project's name, opened as a menu of the others.** *Unconfirmed.*

  ![The new-workspace dialog's menu of projects](screens/new-workspace-projects.png)

- **The dialog's `+` is at its bottom left, as the composer's is, and opens the same menu,** which attaches files and turns plan mode on and off, as `Shift-Tab` does there too, and in plan mode the new workspace's first chat starts planning.

  **The dialog's `+`, opened.** *Unconfirmed.*

  ![The new-workspace dialog's + menu](screens/new-workspace-menu.png)

- **In plan mode the dialog is copper over plan mode's texture, as the composer is.** *Unconfirmed.*

  **A task described in the dialog, in plan mode.** *Unconfirmed.*

  ![The new-workspace dialog in plan mode's copper](screens/new-workspace-planning.png)

- **The prompt can be dictated, with a microphone beside Create or the same shortcuts as the chat composer.** Holding Space or `Option-Space` while the prompt has focus starts dictation, as `chats.md` says. Live words show under the input, and finalized sentences enter at the cursor and save with the shared draft. While dictating, the prompt shows the fine line and recording toolbar described in `chats.md`; Return finishes transcription before creating the workspace; a transcription failure leaves the dialog open with the text already entered. Releasing the key never creates. Escape cancels the recording and restores the starting text before another Escape closes the dialog. Closing it drops tentative words, so reopening it cannot receive a previous recording's text.

  **Dictating the new workspace's first prompt.**

  ![A new-workspace prompt with live dictated words and recording controls](screens/new-workspace-dictation.png)

- **Create is the dialog's main action, and Return presses it.** Shift-Return starts a new line. Create with nothing typed makes the workspace and sends nothing, so making one is still two keys. *Unconfirmed.*
- **Create, once any dictation has finished, closes the dialog at once, and the sidebar shows the workspace being made,** since it fetches first and can take a while. Every window shows it, wherever it was asked for, such as from the command line. Once it's made it's selected, if what showed as Create was pressed still shows. Another workspace, Home or Settings showing in its place stays, since that's where the person went, or where the app went for them, as when the workspace showing was archived from elsewhere. The prompt is its first chat's, sent with the model, effort and plan mode chosen. If it can't be made, what was typed goes back to the dialog with plan mode, and the dialog opens again to show it. A prompt the new chat couldn't be sent goes to its composer, and the workspace is selected even if something else shows by then, so the prompt is there to see. The chat is asked into plan mode if the prompt was to be planned. So nothing typed is lost, and nothing to be planned is sent with full access. *Unconfirmed.*

  **The sidebar while a workspace is being made in `roundhouse`, whose `+` waits.** *Unconfirmed.*

  ![A workspace being made, in the sidebar](screens/creating-workspace.png)

- **The dialog opens while another workspace is being made, and its Create waits until that one is.** A workspace that's made meanwhile is selected behind the dialog, unless the person went elsewhere first, as the line above says, and typing stays in the dialog until it closes. *Unconfirmed.*

  **The dialog, opened while another workspace is being made, its Create waiting.** *Unconfirmed.*

  ![The new-workspace dialog with Create greyed out](screens/new-workspace-waiting.png)

- **The model and effort start as a chat nobody has chosen for would run: Opus at Extra high for Claude, GPT-6.1 Sol at Low for Codex, and Gemini 4 Carbon at Medium for Antigravity,** as `chats.md` says. *Unconfirmed.*
- **Changing the agent resets the model and effort to its defaults,** and keeps that choice with the dialog’s shared draft. *Unconfirmed.*
- **Closing the dialog keeps what's in it for the next time it opens,** whether with Escape or a click outside it. The model and effort stay as chosen for the next workspace too, but plan mode doesn't, since it belongs to the task at hand, as in a new chat. *Unconfirmed.*
- **The hub keeps what's in the dialog, files and all, until the workspace is made,** so it's there when the app opens again, and in any other client, as a chat's draft is, as `chats.md` says. A change made in another client shows as it reaches the hub, unless something from here is on its way, which then replaces it. The project the dialog is opened for takes the place of the one kept. The hub makes the workspace from what it keeps, so a file it kept goes with the prompt even before the dialog has read it back, and keeps what was in the dialog until the new chat has the prompt, so quitting meanwhile loses nothing. Then it empties the dialog, unless something was typed in it since, here or in another client, which stays. A prompt the hub couldn't give the new chat whole, such as one carrying a file it couldn't copy, stays in the dialog, which opens again to show it. *Unconfirmed.*
- **The dialog's header also starts a workspace from another branch or a pull request, and links an issue to it.** *Not built.*

- **A workspace's branches go in a folder of its name, so `euston` starts on `euston/start`,** and the branches made for its work can go beside it, as `euston/try-the-parser`. So a workspace's branches stay together, and one prefix finds them all, in `git branch`, on GitHub and in a stack. Git can't have a branch `euston` beside a folder `euston/`, so the first branch has a name of its own in the folder, and a name git has a branch or a folder of branches by, here or among `origin`'s as last fetched, is taken, as one with a directory is. So is a remote's name, since a branch in a folder named `origin` would be found before `origin`'s own, and so is `head` where the repository has a remote, since on a file system that doesn't tell case apart, a branch `head/start` pushed there can't sit beside the remote's `HEAD`. Git's names count whatever their case, since it keeps a branch as a file, and a file system that doesn't tell case apart takes `Euston` for `euston`. *Unconfirmed:* `origin`'s branches, a remote's name, `head`, and counting names whatever their case. A workspace made before its branches went in a folder keeps the branch it was made on, named `euston`.
- **A workspace made without a name of its own is named after a railway station,** such as `euston`, as a roundhouse is a railway's building. Each is one word of plain letters that's easy to say and to spell, since it's typed as a branch and a directory. It's picked at random from those no other workspace of the project has, archived or not, so names made one after another are seldom alike. When the name is taken after all, because git has it, which the hub learns once the workspace's worker is up, or a workspace made at the same moment has it, the same station gets a number, such as `euston-2`, and the next number nobody has, git included, each time, up to eight names in all, after which the workspace isn't made. Once every station is taken, one is picked at random with the lowest number nobody has with it. *Unconfirmed.*
- **Keeping a workspace's branches in its folder is best effort.** An agent or a person can name a branch anything, and nothing stops git doing it, since what would stop it, a `reference-transaction` hook, would break `gh` and the person's own terminal. Which branches are a workspace's comes from what its worktree has had checked out, whatever they're called, so a branch outside the folder loses nothing, and is only harder to find.
- **Agents make a workspace's branches and other worktrees with Roundhouse's tools, which name them in its folder,** and Claude is reminded to use them rather than git. *Not built.*
- **Setup runs in the background once the worktree exists.** It copies gitignored files matching `.worktreeinclude`, then `file_include_globs`, then `.env*`, and runs `scripts.setup`. A prompt sent meanwhile is recorded at once and delivered when setup finishes. A failure warns, and never blocks. Restoring a workspace runs it again. The matching is git's own, so patterns mean what they do in `.gitignore`. *Unconfirmed.*

  **A new workspace still setting up, with its log open.** *Unconfirmed.*

  ![Setup running](screens/setting-up.png)

  **A new workspace whose setup script failed, with its log open.** *Unconfirmed.*

  ![A failed setup](screens/setup-failed.png)

## Titles

- **The sidebar and a workspace's header show its title,** so telling workspaces apart doesn't mean opening each one. Until it has one, they show its name.

  **Workspaces in the sidebar with titles generated, one from a long prompt's first line, and two with none yet, which show their names.** *Unconfirmed.*

  ![Titles in the sidebar](screens/titles-in-the-sidebar.png)

- **A workspace's title is generated from its first prompt.** Until the title arrives, or if generating it fails, it's the prompt's first line, cut at a word after 48 characters, so a prompt is never held up for it.
- **A title is generated from a prompt's images as well as its words,** since a prompt such as "What do we need to do to resolve this?" is often about a screenshot. The call is given the first three images, up to 32 MB between them, and none of the prompt's other files. A prompt of only images has no first line, so until its title arrives, the workspace shows its name and the chat is Untitled, and if none comes, until a prompt with words. Regenerate title is greyed out meanwhile, as it is for any workspace without a title. *Unconfirmed.*
- **A generated title is a few words in sentence case, with no full stop,** such as "Fix login redirect", so it fits a sidebar row.
- **A title is generated by a call of its own to a small model, not by asking the workspace's agent,** so it costs the agent no context and never shows in its chat.
- **A title can be changed by hand, and one typed by hand is never replaced by a generated one that nobody asked for.**
- **A title is one line of at most 120 characters,** so a pasted paragraph can't fill a row. Line breaks become spaces, and characters that can't be seen or that reorder text are left out, so a row reads as the title does.
- **A workspace from before titles takes the title of its oldest chat,** which is that chat's first prompt's first line, rather than whatever is said in it next.
- **A new title leaves the workspace's name alone.** The name, such as `euston`, is the worktree's directory, the folder its branches go in, and `CONDUCTOR_WORKSPACE_NAME`. Renaming a worktree under a running agent would break its working directory, and any absolute paths setup made.
- **Rename… is in the menu a right-click opens, on a workspace in the sidebar or a chat's tab,** and asks in a dialog, with the title selected so typing replaces it.

  **Renaming a workspace, with every workspace titled.** *Unconfirmed.*

  ![Renaming a workspace](screens/rename.png)

- **Regenerate title asks for a new title from everything asked in the workspace,** since the work can move on from its first prompt. It's under Rename… in the menu a right-click on a workspace's row opens, and a button beside the title in the workspace's header. Both have Rename…'s pencil, with sparks, so it reads as renaming that the app does. A title, branch or base too long for the header ends in an ellipsis, so the button is never pushed out of view. *Unconfirmed.*

  **A title too long for the header, ending in an ellipsis before Regenerate title's button.** *Unconfirmed.*

  ![A long title in the header](screens/long-title.png)

- **The new title names the work as a whole, from every chat's prompts, oldest first,** by a call like the one for the first title. It names one thing, and never a chore such as a review or merging, which most workspaces end with and a model given only the newest prompt would name. Of each prompt it's given the start, and of a long history the first prompt and the newest, so the call stays as small as the first. It's given the oldest of their images, by the first title's limits, since the first prompt's set the task. *Unconfirmed.*
- **A new title that's asked for replaces one typed by hand too, but not one set while it's on its way,** so renaming by hand always wins over a call that's slow to answer. *Unconfirmed.*
- **While a new title is on its way, the workspace's row says Renaming… and the header's button spins,** so it's clear the press took, and neither asks again. If none comes, such as when the agent's CLI isn't logged in, the app says why and the title stays. *Unconfirmed.*

  **Waiting for a new title, asked for from the header.** *Unconfirmed.*

  ![Waiting for a new title](screens/regenerating-title.png)

  **Waiting for a new title, as the sidebar shows it.** *Unconfirmed.*

  ![Renaming… in the sidebar](screens/renaming-in-the-sidebar.png)

- **Until a workspace has a title, Regenerate title is greyed out,** since nothing has been asked in it to title it from. One renamed by hand before anything is asked has a title and nothing to make another from, so asking says so and changes nothing. An archived workspace isn't titled again. *Unconfirmed.*

  **The menu of a workspace nothing has titled yet.** *Unconfirmed.*

  ![A workspace's menu with Regenerate title greyed out](screens/untitled-workspace-menu.png)

## Where a workspace is

**Resting the pointer on a workspace's name in its header shows where it is,** in a card under the name: the path of its worktree in the code font, cut at its start if it's too long, since its end is the workspace's own folder, with a button that copies it whole, then under a rule, the version its agents run on, where they run, and the ten ports set aside for it, the first of which is `CONDUCTOR_PORT`. One worker does all of a workspace's work, every chat's included, so the card is the workspace's rather than a chat's. The card goes a third of a second after the pointer leaves the name, as the context's card does, so the pointer can cross to the button, and stays while it's on the card. What isn't known is left out, such as the path before the worktree is made. A workspace still on a version before the app's says so, as `updates.md` shows. *Unconfirmed.*

![The card a workspace's name opens](screens/workspace-card.png)

## Settings from Conductor

A repository set up for Conductor works here unchanged, because Roundhouse reads its settings and sets its variables under Conductor's names. There are no names of Roundhouse's own for these.

- **Settings:** `.conductor/settings.local.toml` in the repository, then `.conductor/settings.toml` in the worktree, then `~/.conductor/settings.toml`. The first to set a value wins.
- **Scripts:** `[scripts]` with `setup` and `archive`. Run scripts, `run_mode` and managed settings aren't read.
- **Files to copy:** `.worktreeinclude`, then `file_include_globs`, then the default `.env*` pattern.
- **Variables** for scripts and agents: `CONDUCTOR_WORKSPACE_NAME`, `CONDUCTOR_WORKSPACE_ID`, `CONDUCTOR_WORKSPACE_PATH`, `CONDUCTOR_ROOT_PATH`, `CONDUCTOR_DEFAULT_BRANCH`, `CONDUCTOR_PORT`, the first of ten ports for the workspace, and `CONDUCTOR_IS_LOCAL`.

## Archiving

- **Archiving removes the worktree and keeps the branch,** following it if the agent renamed it. It refuses if there are uncommitted changes, or commits on no branch, unless forced. That's checked before anything changes, and again once the agents have stopped. Ignored files such as `.env` and `node_modules` go with the worktree. Committed work can't be lost, and uncommitted work can't be lost by accident. *Unconfirmed.*
- **What's in the worktree's `.context/` is ignored, so it goes with the worktree too,** since it's where agents keep what they make for their own use, and where a prompt's files go, as `chats.md` says. A repository that keeps a `.gitignore` of its own in `.context/` decides what's ignored there instead. *Unconfirmed.*
- **Archive is on a workspace's row, in the sidebar and in Home, and on its pull request's bar once the pull request is merged or closed,** where it's the next thing to do. `Cmd-Shift-A` archives the workspace showing, as `sidebar.md` says, and so does File > Archive Workspace, as `menus.md` says. *Unconfirmed:* the File menu's.
- **Archiving is done once the worktree is out of the way, and its files are deleted after,** since a build's output can be tens of thousands of files, which take seconds to delete. Moving the worktree aside on its own disk is instant, and frees its path and branch, so it can be restored at once. One with submodules is refused unless forced, as git refuses it, since a submodule's own commits are in it. *Unconfirmed.*
- **A worktree that can't be moved aside is refused, and left as it was,** such as a disk of its own, or on Windows, one a program has a file open in, such as a hub built there and still running. It names each program running from the worktree, the likeliest to have one, so it's clear what to stop. Deleting it where it is could stop part way, and git forgets a worktree it was deleting even then, which would leave half of one.
- **A worktree's folder that git has already forgotten is deleted only when forced,** since git can't say whether what's left of it holds work. Its branch is kept, as always. *Unconfirmed.*

## Other worktrees

- **The worktrees a workspace's agents make for its work besides its own are the workspace's,** so Roundhouse makes and records each one, and archiving the workspace removes them with it, rather than each lasting until an agent remembers to remove it. An agent makes one for a subagent, or for any work that needs a checkout of its own. *Not built.*
- **A workspace's other worktrees go in `.children/` beside the workspaces, in a folder named after the workspace,** so `euston/try-the-parser`'s is `.children/euston/try-the-parser`. No workspace's name starts with a dot, and none is inside the workspace's own worktree, where it would show in `git status` and go along when archiving moves the worktree aside. *Not built.*
- **Each is in a directory of one name: its branch's, without the workspace's folder, with each `/` left in it written `-`,** so `euston/x/y`'s is `.children/euston/x-y`, and `fix/login`'s, a branch outside the folder, is `.children/euston/fix-login`. Laid out as its branch is, one worktree would be made inside another, as `euston/x/y`'s would be inside `euston/x`'s. *Not built.* *Unconfirmed.*
- **A new one is on a new branch in the workspace's folder, from the commit the workspace's worktree has checked out unless the agent gives a base, or on one of the workspace's branches as it is.** A name that's taken gets `-2`, `-3` and so on, so the agent never has to choose again. *Not built.*
- **A workspace made before its branches went in a folder names a new branch beside its own, as `euston--try-the-parser`,** while git still has its branch `euston`, or its worktree has had it, since git can't have `euston/…` beside `euston`. Its worktrees go in the same folder of `.children/` either way. *Not built.* *Unconfirmed.*
- **Ignored files are copied into a new one as setup copies them,** since copying is quick. *Not built.*
- **The setup script runs in a new one only when the agent asks for it,** since a subagent that doesn't build shouldn't wait for setup. *Not built.*
- **Archiving refuses if any of a workspace's other worktrees has uncommitted changes or commits on no branch, naming which, as it does for the workspace's own, unless forced.** Then it removes them all, and keeps every branch, since squashed merges leave no way to tell that a branch's work has landed. *Not built.*
- **Restoring brings back only the workspace's own worktree.** The others' branches are kept, and a worktree can be made on one again. *Not built.*

## Pushing to GitHub

- **Whether agents can push to a workspace's repository on GitHub as the default GitHub account is the repository's, so every workspace of it says the same, as the hub last found it,** since what GitHub takes depends on the repository and the account, not the workspace. The hub finds out by asking GitHub, as "Reach" in "State ownership" in `docs/architecture.md` says.
- **A workspace that agents can't push from says so in a line above the composer, with why and what to do,** in every chat of it, so it's seen before work starts and when a push fails. It blocks nothing: agents push as they would anyway. *Unconfirmed:* the line above the composer.
- **It says what's wrong in a warning's colour, then GitHub's own words, or the hub's, and what to do, quieter, on from it as one paragraph:** choose another default or connect an account that can push there, or, for a token's or a sign-in's account, ask the repository's owner to give it access or to approve Roundhouse, as an organization that restricts OAuth Apps has to. *Unconfirmed.*
- **`Open Git`, at the line's end, opens Settings at Git,** where another default is chosen and accounts are connected, as `settings.md` says, so what the line says to do is a click away. A line whose fix is in the repository's own git config, below, has none. *Unconfirmed.*
- **For a default that's an app of your own's installation, the line names the app's bot, such as `ada-roundhouse[bot]`, as GitHub knows it, and for a repository of the owner the app is installed on, says to install the app there with access to the repository, with `Install on GitHub`, before `Open Git`, which opens GitHub's page for installing the app,** where an app that's installed already is given more repositories. Its bot reaches only the repositories the app is installed on and given there, and approving Roundhouse is for a sign-in, so the line asks the repository's owner for neither. For a repository of another owner, it says which owner the app is installed on, so the bot can't push to the other's repositories, and to choose another default or connect an account that can push there, with no `Install on GitHub`, since an app of your own is installed only on its own owner. When GitHub doesn't take its token, it says to install the app there again and connect it again, with `Install on GitHub`. *Unconfirmed.*

  **A workspace whose repository GitHub refuses the default's pushes to, for an app of your own's installation on the repository's owner that wasn't given it.** *Unconfirmed.*

  ![A line above the composer saying GitHub refuses pushes as the app's bot, with Install on GitHub](screens/push-refused-app.png)

- **It says so when GitHub refuses the push, in GitHub's words, when GitHub has no such repository the account can see, when GitHub doesn't take the account's token, and while the default's login can't be used, in the hub's words, which say what to do.** It says nothing while a push would be taken, while the hub couldn't tell, as while GitHub can't be reached and hasn't answered for the account before, since a push that fails then says why itself, while the default is each machine's own login, which the hub holds no token to check with, and for a repository whose `origin` isn't on GitHub. *Unconfirmed.*

  **A workspace whose repository GitHub refuses the default's pushes to, in GitHub's words.** *Unconfirmed.*

  ![A line above the composer saying GitHub refuses pushes as the default](screens/push-refused.png)

  **A workspace whose repository GitHub has none of that the default can see, as for a private one that isn't theirs.** *Unconfirmed.*

  ![A line above the composer saying GitHub has no such repository the default can see](screens/push-not-found.png)

- **A workspace whose repository's pushes go round the default says that agents push there as their machine's own login, and how to push as the default,** since where `origin` pushes, as git rewrites it, isn't `https://github.com/`, where the git credential the hub hands workers is what git pushes with. So it goes over SSH with the machine's key, as a person's `url."git@github.com:".insteadOf "https://github.com/"` has an `https://` address go, and as `ssh://github.com/…` and an address with a port do, or to another host, or with a password of its own in the address. It says so whatever the default, as while its login can't be used, which such a push never meets, but not while the default is each machine's own, since that's who pushes then anyway. The hub asks GitHub nothing of it. Its fix is to have `origin` push to `https://github.com/`, in the repository's git config, so it has no `Open Git`. *Unconfirmed:* the words, and that it has no button.

  **A workspace whose repository's pushes go round the default, as over SSH.** *Unconfirmed.*

  ![A line above the composer saying agents push to the repository as their machine's own login](screens/push-as-machine-own.png)

- **The line goes once the hub finds that a push would be taken,** as when another default is chosen, which the hub checks at once, or an organization approves Roundhouse, which it finds within half an hour. *Unconfirmed.*
- **What GitHub last answered stays until GitHub answers again,** so a hub that starts again shows the line its last run left, and checks at once, and while GitHub can't be reached, the line says what GitHub last said for the same account. It looks the same either way, since the hub checks again as it starts, so it's seldom out of date for long. *Unconfirmed:* no sign of an answer the last run left.

## Pull requests

- **The top of the sidebar of changes is the pull request's bar,** level with the workspace's header: the pull request's number, which opens it on GitHub, the first thing in the way of merging it, and the one thing to do about it. The whole bar takes the pull request's colour, as `look.md` says, so where it stands reads at a glance from across the window. While the sidebar is hidden, the header shows the number and the action instead, since it has no room for the words. *Unconfirmed.*

  **Every state of the pull request's bar, as the sidebar of changes shows it, and beside it what the header shows while the sidebar is hidden. Merged shows three times: the second while another workspace is being archived, and the third while this one is.** *Unconfirmed.*

  ![Every state of the pull request's bar](screens/pull-request-states.png)

- **The pull request's bar fills its main action with the pull request's colour,** still with dark text, so the action reads as part of the bar it's on. One with no colour of its own, such as Create PR, is the light main button. *Unconfirmed.*
- **The bar says the first thing in the way, in this order:** merged or closed, a place in the merge queue, then what's here and not on GitHub, which is uncommitted files and then commits to push or pull, then what GitHub can't merge with, which is conflicts with the base, changes requested, a draft and failed checks, then what it's waiting on, which is checks running, a review, the repository's other rules and GitHub still working it out, and ready to merge once nothing is. Done comes first because nothing else matters then, and the queue next because it merges the pull request whatever else is true. What's here comes before what GitHub says, because GitHub's answers are about an older commit until it's pushed. *Unconfirmed.*
- **Each check counts once, as its latest run,** since GitHub keeps the runs that a newer push or a rerun replaced, and counting those would call a pull request whose checks passed failing. *Unconfirmed.*
- **A pull request whose newest commit isn't in the worktree has at least one commit to pull,** such as one pushed from elsewhere, until a fetch says how many. *Unconfirmed.*
- **What this version doesn't know shows as neither good nor bad news,** in the theme's colours, with "See it on GitHub" and nothing to do. That covers a word GitHub adds and a hub too old to say how the pull request can merge, either of which a guess could call ready. *Unconfirmed.*
- **Its action asks the agent, as `Create PR` does,** in the chat showing, which it then shows: Commit, Push, Pull, Sync, Resolve, Address, Ready for review, Fix checks or Merge. The agent knows why the changes were made and how the repository works, such as how it merges and whether through a queue. A merged or closed pull request offers Archive, and one waiting on GitHub or on someone else offers nothing. *Unconfirmed.*
- **`Create PR` asks the agent to commit, push and run `gh pr create`,** and the app shows the result from `gh pr view`. The agent knows why the changes were made, so it writes the better description. *Unconfirmed.*
- **A workspace's pull request is the one from the branch on GitHub its branch is pushed to,** since a pull request is from a branch on GitHub, which is one repository's by one name, where a branch on a computer has whatever name that computer gives it, and each computer names its workspaces from the same list. So two workspaces on the same branch, as when two agents are asked to work on one, show the same pull request, and one on a branch of the same name pushed somewhere else, such as a fork, doesn't show another's. An open one shows first, then the one the workspace had, once it has merged or closed. One that has merged or closed, seen for the first time, shows only if its newest commit is where the branch or its upstream is, since GitHub keeps the pull requests of a branch whose name is used again once the one before was merged and deleted. A branch pushed nowhere yet has none. *Unconfirmed.*
- **The pull request is read again each time its workspace is shown, when a turn ends, and every 30 seconds while it shows, unless it's merged or closed,** because checks, reviews and the queue move on GitHub while nobody's looking, and a workspace with none gets one when the agent or someone on GitHub opens it. It's also read when the worktree checks out a branch other than the one it was last found on, merged or not, since a pull request is its branch's, and an agent often starts on its next piece of work on a new branch in the middle of a turn, and when the branch checked out is pushed to its upstream, though not when a fetch or a pull moves the branch it tracks. Until the first answer, the bar shows what the hub last found, or says it's looking if the hub hasn't looked, rather than offering to create a pull request the workspace may already have. Only the newest answer shows, since an older one can arrive after it, and a failure to ask keeps what was known. *Unconfirmed.*
- **The hub keeps each workspace's pull request, and reads every one again on its own,** so each row in the sidebar shows where its pull request stands without being opened. What it found is kept across restarts, so an app that opens again, such as after an update, shows every row's at once, and GitHub failing, such as while offline, never takes away a pull request already found. Only GitHub saying the branch has none does, or only ones that aren't the workspace's. Where it can't be read at all, without a login to GitHub or a repository on github.com, a workspace with none found offers to create one.
- **Reading them stays within what GitHub can spare,** since its hourly limit is shared with every agent's own `gh`. Every workspace due as the same login is read together, in a few queries, however many there are. The workspace showing, a turn that just ended and a branch that's moved go at once. A branch that's moved is the agent's doing, which it can do over and over, so its read waits until ten seconds after the workspace's last. It goes by what GitHub says is left of each login's limit, and leaves the last fifth of it to everything else, which a read someone is waiting on can still use until none is left, though not one for a branch that's moved. A pull request that keeps coming back the same waits longer each time, from two minutes up to half an hour, as does one that can't be read, such as while offline, and one with checks running or in the merge queue waits the least. Reads nobody is waiting on stop while no app is connected, since nobody would see them, though a turn that ends or a branch that moves still reads its own. A merged or closed pull request waits to be shown, unless it was found on another branch than the one checked out, since then it isn't the workspace's.

## Waiting on pull requests

- **An agent is told to use Roundhouse's tools rather than `gh` or a loop of its own whenever what it does next depends on where a pull request stands,** such as on its checks, the merge queue or its merge, and, where they can wait, to wait for its checks or its merge with them and end its turn. So the wait shows in its chat, with Stop beside it, and the person can talk to the agent meanwhile, where waiting with `gh` holds its turn with only a command to show. Codex's and Antigravity's tools can't wait, so they're told only to look among them. *Unconfirmed.*
- **An agent can wait on explicit pull requests and their open lower GitHub stack dependencies,** using `wait_for_pull_requests` with `pull_requests` and `until`. `get_pull_requests` reads the same scope. The numbers belong to the workspace's repository, and a wait keeps that repository even if the worktree's remote or checked-out branch changes. It follows the latest pushed revisions, without changing the worktree.
- **The agent chooses required checks, all reported checks, or merging.** Every watched PR must satisfy the condition; one conflict, required rebase, relevant failed check, new request for changes, closure without merging or queue rejection ends early. Required checks are those effective at the stack's destination, so optional failures do not end that condition. Missing expected checks and unknown state cannot pass. Drafts allow checks waits. Entering a merge queue is not merging, and its transient conflicts do not interrupt its work.
- **Each wait is independent.** Starting one does not replace another, and retrying its tool call reuses it. Stop ends only the wait beside it. Closing the chat or archiving its workspace ends all its waits. A wait called off by the person asks the agent not to recreate it unless asked.
- **Moving main alone does not end a wait.** A lower PR merging and the expected retargeting continue it. Inserting, replacing or reordering a lower dependency, or changing the stack's destination, ends it with the changed scope. Adding a PR above the requested one does not broaden the wait.
- **A wait can have a deadline, and otherwise has none.** The hub keeps it across restarts and checks the deadline even when GitHub cannot be reached. A temporary read failure leaves the last observation and says when it was read and what failed. Losing access ends the wait as unavailable. The result says whether its condition was reached, attention is needed, time ran out, access was lost or it was cancelled, with the affected PRs, revisions and check links.
