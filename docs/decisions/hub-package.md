# A hub on a machine of its own is installed from an archive as a service, and a client updates it

**Status:** Decided. All but the app's card for updating the hub is built.
**First recorded:** 2026-11-20, in #2119.

A hub that runs on a machine with no desktop app, such as a server, is installed from an archive of its own. Each release publishes one for each OS and processor, listed in the signed update feed beside the app's packages. It holds `roundhoused`, `roundhouse`, and the Linux workers a provider puts in its instances.

`roundhouse hub install` registers the hub with the machine's service manager, which starts it at boot as the user it's given, with nobody logged in. On a Mac that's a launch daemon, and on Linux a systemd service. The hub's files belong to that user, with each version beside the last.

The hub updates only when a client says. It reads the feed itself and tells its clients when a newer version is out. When someone presses the client's button, the hub downloads that release, checks it against the feed's signature, unpacks it beside the running version, and exits as a failure, so the service manager starts the new version. The next hub removes the versions nothing runs from.

## Chosen over

- **The desktop app on the server, keeping its hub running.** A server would need someone logged in, and an app nobody looks at. The app updates its hub only as it updates itself.
- **A bundle Velopack makes and updates.** On Linux, Velopack makes only an AppImage, which a systemd service or a container runs awkwardly. On a Mac it wraps the hub in an app bundle. The service would still need registering.
- **Package managers, such as Homebrew and apt.** They own the files, so the hub couldn't update itself when a client says. Every merge publishes a release, which would outrun their caches.
- **The hub updating itself as soon as a release is out.** A hub that runs ahead of its clients cuts off a client two protocol breaks behind it, as a hub left behind is cut off by its clients. So the person using them chooses when.
- **The service manager always starting the hub again.** Every install would need installing again first, since the hub's user can't change its service.
- **The hub replacing its own process with the new version,** which works only on Unix.

## What it costs

- **The hub has an updater of its own,** apart from Velopack's, which downloads the whole archive rather than what changed. It shares the app's check of the feed's signature, and its choice of a release that crosses one protocol break at a time.
- **A hub nobody updates falls behind its clients,** and two protocol breaks behind, they can't reach it, as [only the hub speaks the previous version](hub-speaks-previous-version.md) says.
- **A paired device can restart the hub, by updating it,** though it can't stop or restart it otherwise.
- **Installing needs an administrator once,** to register the service. Updating needs none.
- **The service manager logs each update as a failure,** which systemd counts toward its start limit.
- **Agents outlive an update only where workers outlive their hub,** which is the local provider's, on a Mac and Linux. A provider's adapter is the hub's child, so it restarts with the hub, and its workers stop their agents when they lose the hub. On Linux, the service sets `KillMode=process`, since systemd otherwise stops every process the service started.
