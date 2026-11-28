# Install

Roundhouse runs on a Mac with Apple silicon. Windows and Linux have first previews.

## What you need

- **Git, and a repository to work in.**
- **Your agent's CLI, `claude`, `codex` or `agy`, installed and signed in.** Run it once in a terminal to check. Claude can also use a login added in [Settings](../reference/settings.md#agents). Codex and Antigravity use their own sign-ins on the machine running the workspace.
- **GitHub's CLI, `gh`,** for pull requests, with a GitHub account added in [Settings](../reference/settings.md#git), or logged in. Everything else works without it.

## On a Mac

1. [Download Roundhouse](https://f001.backblazeb2.com/file/getroundhouse-releases/Roundhouse-macos-arm64-Portable.zip) and unzip it.
2. Move it to Applications, and open it.

It's signed and notarized, and it updates itself as changes are published. [Updates](../reference/updates.md) says what an update does to running agents, which is nothing.

The first time an agent needs you while you're elsewhere, macOS asks whether Roundhouse may show notifications.

## On Windows

A first preview. Download the installer for [amd64](https://f001.backblazeb2.com/file/getroundhouse-releases/Roundhouse-win-amd64-Setup.exe) or [ARM64](https://f001.backblazeb2.com/file/getroundhouse-releases/Roundhouse-win-arm64-Setup.exe) and run it. It installs for you alone, without an administrator, and updates itself from then on.

> [!WARNING]
> The installer isn't signed, so Windows warns before it runs. The only checks that run on Windows are a lint, a hub starting and stopping, and a workspace's agent carrying on as the hub restarts.

What differs from the Mac:

- No sound plays when an agent needs you.
- Nothing keeps the computer awake while agents work.
- There's no dictation.
- The app's keys have `Ctrl` where a Mac's have `Cmd`, as [Keys](../reference/keys.md) says.
- The menus open from the first button in the window's top left corner, or `F10`. Settings… and Exit are in File, and About Roundhouse and Check for Updates… in Help.
- A repository's setup and archive scripts run in Git for Windows' `bash`, which its installer puts beside `git`.

## On Linux

A first preview. Download the AppImage for [amd64](https://f001.backblazeb2.com/file/getroundhouse-releases/Roundhouse-linux-amd64.AppImage) or [ARM64](https://f001.backblazeb2.com/file/getroundhouse-releases/Roundhouse-linux-arm64.AppImage), make it executable, and open it:

```sh
chmod +x Roundhouse-linux-amd64.AppImage
./Roundhouse-linux-amd64.AppImage
```

It updates itself. It needs glibc 2.35 or newer, as on Ubuntu 22.04 and later, Vulkan, and FUSE.

What differs from the Mac:

- The app's keys have `Ctrl` where a Mac's have `Cmd`, as [Keys](../reference/keys.md) says.
- The menus open from the first button in the window's top left corner, or `F10`. Settings… and Quit are in File, and About Roundhouse and Check for Updates… in Help.
- A notification needs a notification service, and no sound plays.
- Nothing keeps the computer awake while agents work.
- There's no dictation.

## A hub on a machine of its own

A Mac or a Linux machine with no app on it, such as a server, can run a hub that starts at boot, with nobody logged in, for the app on your other machines to [connect to](../guides/connect-another-hub.md). Download the hub for a [Mac](https://f001.backblazeb2.com/file/getroundhouse-releases/Roundhouse-macos-arm64-hub.tar.gz), or for Linux on [amd64](https://f001.backblazeb2.com/file/getroundhouse-releases/Roundhouse-linux-amd64-hub.tar.gz) or [ARM64](https://f001.backblazeb2.com/file/getroundhouse-releases/Roundhouse-linux-arm64-hub.tar.gz), unpack it, and install it to run as the user whose files and credentials its agents should use:

```sh
mkdir roundhouse-hub
tar -xzf Roundhouse-macos-arm64-hub.tar.gz -C roundhouse-hub
sudo roundhouse-hub/roundhouse hub install --user <name>
```

It says how to pair a device with it once it's running. It looks for a newer release every 10 minutes, and the app on a device connected to it shows a card when it has found one, whose `Update hub` [updates it](../reference/updates.md#a-hub-on-a-machine-of-its-own). Agents at work keep going. On its machine, `roundhouse hub update`, run as its user, does the same. Installing a newer download the same way updates it too. [The command line](../reference/command-line.md#the-hub) says what else `roundhouse hub` does.

## No account

There's no Roundhouse account to make. The app's only requests of its own are its check for updates and, once you turn dictation on, the download of the model that hears it. Everything else stays on your machine, or goes where your agent, `git` and `gh` send it. The hub also asks GitHub about the GitHub accounts you add, as [Where data is kept](../reference/where-data-is-kept.md#what-leaves-your-machine) says.

Next: the [quickstart](quickstart.md), from adding a project on.
