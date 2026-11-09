# Updates

What someone using the desktop app sees when it or its hub updates, or when there is no hub to reach. How it works is under "Managing a hub" in `docs/architecture.md`.

- **Updating never stops running agents unless you choose to.** An update looks like this:
  1. The app updates and relaunches. Everything shows as before, and running agents keep going on the old hub. *Not built:* the app downloads an update in the background and installs it when you quit, without relaunching.
  2. If nothing is running and no other client is connected, the app replaces the hub right away. Otherwise it waits until the next launch. *Not built:* the app only replaces a hub when it connects.
  3. Only a real protocol break shows a screen, with how many agents are running and a way to restart the hub.
- **You can restart an older hub yourself rather than wait.** While the hub is older than the app and wasn't started by hand, the app offers to restart it, and says how many agents and setup scripts that would stop. Each stopped agent picks up its conversation at its next prompt. *Not built.*
- **An older hub keeps working.** The app hides or disables what it doesn't support, and only a real protocol break stops it.
- **A hub someone runs themselves is left alone,** even on the same Mac. The app says when it's older, and only its admins stop it.
- **A failed check or download says so under the build in the sidebar,** with the reason on hover, until a later check works. *Unconfirmed.*

## When there's no hub

**The app couldn't reach a hub, or start one.** *Unconfirmed.*

![No hub](screens/no-hub.png)
