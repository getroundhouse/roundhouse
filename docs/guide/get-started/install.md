# Install

Roundhouse runs on a Mac with Apple silicon. Windows and Linux have first previews.

## What you need

- **Git, and a repository to work in.**
- **The Claude CLI, `claude`, installed and logged in.** Roundhouse runs it as you, with your login and settings. Run `claude` once in a terminal to check.
- **GitHub's CLI, `gh`, logged in,** for pull requests. Everything else works without it.

## On a Mac

1. [Download Roundhouse](https://f001.backblazeb2.com/file/getroundhouse-releases/Roundhouse-osx-Portable.zip) and unzip it.
2. Move it to Applications, and open it.

It's signed and notarized, and it updates itself as changes are published. [Updates](../reference/updates.md) says what an update does to running agents, which is nothing.

The first time an agent needs you while you're elsewhere, macOS asks whether Roundhouse may show notifications.

## On Windows

A first preview. Download the installer for [amd64](https://f001.backblazeb2.com/file/getroundhouse-releases/Roundhouse-win-amd64-Setup.exe) or [ARM64](https://f001.backblazeb2.com/file/getroundhouse-releases/Roundhouse-win-arm64-Setup.exe) and run it. It installs for you alone, without an administrator, and updates itself from then on.

> [!WARNING]
> The installer isn't signed, so Windows warns before it runs. The only checks that run on Windows are a lint, and a hub starting and stopping.

What differs from the Mac:

- Agents stop when the hub does, so an update waits until nothing is running.
- No sound plays when an agent needs you.
- Nothing keeps the computer awake while agents work.
- The app's keys have `Ctrl` where a Mac's have `Cmd`, as [Keys](../reference/keys.md) says.
- The menus open from the first button in the window's top left corner, or `F10`. Settings… and Exit are in File, and About Roundhouse and Check for Updates… in Help.

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

## No account

There's no Roundhouse account to make. The app's only request of its own is its check for updates. Everything else stays on your machine, or goes where your agent and `gh` send it.

Next: the [quickstart](quickstart.md), from adding a project on.
