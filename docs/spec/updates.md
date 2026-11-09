# Updates

What someone using the desktop app sees when it or its hub updates, or when there is no hub to reach. How it works is under "Managing a hub" in `docs/architecture.md`.

- **Updating never stops running agents unless you choose to.** An update looks like this:
  1. The app downloads an update in the background and installs it when you quit, without relaunching. Everything shows as before, and running agents keep going on the old hub. *Unconfirmed:* `Restart to update` installs it sooner, and opens the app again.
  2. If nothing is running and no other client is connected, the app replaces the hub right away. Otherwise it waits until that's true, then replaces it once its window isn't in front, so it doesn't interrupt anyone, or at the next launch. It looks again whenever an agent finishes, when you go to another app, and every half a minute. *Unconfirmed:* waiting while the app stays open, rather than only until the next launch.
  3. Only a real protocol break shows a screen, with how many agents are running and a way to restart the hub.
- **You can restart an older hub yourself rather than wait.** While the hub is older than the app and wasn't started by hand, the app offers to restart it, and says how many agents and setup scripts that would stop. Each stopped agent picks up its conversation at its next prompt. *Unconfirmed:* `Finish now`, at the bottom of the sidebar, does it, as "The build line" below shows.
- **An older hub keeps working.** The app hides or disables what it doesn't support, and only a real protocol break stops it.
- **A hub someone runs themselves is left alone,** even on the same Mac. The app says when it's older, and only its admins stop it.
- **A problem updating says so under the build in the sidebar,** with the detail on hover, until a later check works. A feed or package that isn't the one the release workflow signed says "Update refused" at once. Not reaching the update server only shows once it has lasted a day, since being offline for a while is ordinary, unless you check for updates yourself. An app that can't replace itself where it is says so when it opens, and one that macOS runs from where it was downloaded asks to be moved to Applications. *Unconfirmed.*
- **Checking for updates answers in the sidebar, never in a native dialog.** An update you asked for shows as the same card as one that downloaded by itself.
- **Check for Updates… in the app menu looks right away,** rather than waiting for the next look, which comes when the app opens and every hour. The sidebar says it's checking, under the build, until the look finishes. A newer version shows as its card. What went wrong shows as a problem at once, even not reaching the update server, because you asked. Otherwise "Up to date" shows under the build for a few seconds. A build that doesn't update itself says so the same way, and one that can't update where it is already says why. *Unconfirmed.*

## The build line

The bottom of the sidebar shows which build is running, faint, and only brings up the hub when it isn't the app's build. While an update has something to say, a card with the app's icon takes the version's place, saying where it's got to and what to do next, with the version on hover. Nothing about updating is a modal or a notification, since every merge publishes one. *Unconfirmed:* all of this section, and each screen in it.

**An update is downloaded.** The card says "Update ready" with the version, and `Restart to update` is its main action. That installs it and opens the app again, and agents keep running, since the hub outlives the app. Quitting installs it too. Anything typed and not sent would be lost, so it asks first when there is.

![Update ready](screens/update-ready.png)

**The app has updated, and agents are still running on the previous version.** The card says "Finishing update" and how many agents it's waiting on, or "Waiting on other work" when what's left is a setup script or something else connected to it, such as the command line, and the app restarts the hub once that's done and its window isn't in front. `Finish now` is a quiet button, since it stops them: it asks "Finish the update now?" and says what it stops, such as "This stops the 1 agent still running on the previous version. The agent picks up its conversation at its next prompt." `Not now` and Escape leave it running.

![Finishing the update](screens/finishing-update.png)

**A hub someone runs themselves is another build.** The version says so, as "1.0.0 · hub 0.9.0", and the app leaves it alone.

![A hub someone runs themselves](screens/own-hub.png)

**A problem updating** shows under the version, muted, as the rule above says. A download that isn't what the release workflow signed is red, since it may be someone tampering with the update.

![A problem updating](screens/update-problem.png)

## When there's no hub

**The app couldn't reach a hub, or start one.** *Unconfirmed.*

![No hub](screens/no-hub.png)
