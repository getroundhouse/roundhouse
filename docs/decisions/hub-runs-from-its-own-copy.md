# A hub that a client starts runs from a copy of the daemon outside the app's install directory

**Status:** Decided.
**First recorded:** 2026-11-09, in #215. The copy's name was added on 2026-11-13, in #981.

When a client starts a hub, it copies the daemon out of the app's install directory, once for each build, and starts the hub from the copy. The copy is named `Roundhouse Hub`, which is what `ps`, Activity Monitor and 1Password's prompt to use an SSH key then call the hub.

## Chosen over

- **Running the hub from the install directory.** An update replaces that directory, so the hub would start its next workers from different code. On Windows, Velopack first kills every process running from it, which would stop the hub and its agents. On Linux, the AppImage's files exist only while the app runs.
- **Making the copy an app registered with macOS.** That would put the app's icon on 1Password's prompt. But 1Password holds back a prompt from an app that isn't in front, and a hub never is. So an agent's push would wait unseen and then fail.

## What it costs

- Every update leaves a copy behind, so a client that starts a hub removes the copies that no process runs from, once they're a day old.
- The Windows firewall lets a program listen by its path, so it would ask again about a hub with a paired device after every update. So on Windows the hub runs from one fixed path, a hard link to its build's copy.
