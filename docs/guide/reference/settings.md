# Settings

The app's own settings. A repository's are in [Repository settings](repository-settings.md).

`Cmd-,`, or Settings… in the app menu, or in File on Windows and Linux, shows Settings in place of the sidebar and the workspace. `Back`, Escape or `Cmd-W` goes back to where you were.

![General, the first page of Settings](../../spec/screens/settings.png)

- **A setting takes effect at once, and lasts.** There's no Save button.
- **A search at the top finds a setting** by what it's called or what it does. `Cmd-F` puts the cursor in it.

## General

| Setting | What it does | Starts |
|---|---|---|
| Always expand thinking | Shows what the agent thinks in full, rather than as a line to open | Off |
| Keep computer awake while agents are working | Stops the Mac going to sleep by itself while there's work | On |
| Skip Claude's safety checks | Lets the few commands Claude still asks about with full access run without asking, such as a removal of a path it can't see. Plan mode and questions still ask | Off |

Work is a turn, something an agent left running in the background, or a workspace being set up. An agent that's only waiting on you isn't, since it could wait all night. The display still sleeps, and closing the lid or a low battery still puts the Mac to sleep.

## Notifications

| Setting | What it does | Starts |
|---|---|---|
| Notify when an agent needs you | Shows a notification as a chat comes to need you | On |
| Play a sound | Plays a sound at the same moments, whether or not a notification shows | On |
| Sound | Which sound: Chime, the app's own, or one of the computer's alert sounds | Chime |

![The Notifications page](../../spec/screens/settings-notifications.png)

A chat needs you when it has a reply you haven't read, when it's waiting on you for an answer or an approval, or when its agent stopped short.

- **A notification says why:** `Finished`, `Stopped short` or `Waiting on you`, with the workspace and the chat. Clicking it shows the chat.
- **The chat you're looking at never notifies,** unless another app is in front.
- **A notification lasts as long as its chat needs you.**
- **The sound plays at most once in two seconds,** so several chats coming to need you together sound once.
- **The app plays the sound itself,** so a Mac's Focus doesn't silence it.
- **Only what happens while the app is open is told of.**

On Windows and Linux no sound plays.

## Whose setting it is

A setting belongs to whatever carries it out:

- **How the app looks and behaves is the app's,** kept on this machine.
- **What the hub does with the app closed is the hub's,** such as keeping the computer awake. The hub keeps it, and it can only be changed while the hub is there to ask.
