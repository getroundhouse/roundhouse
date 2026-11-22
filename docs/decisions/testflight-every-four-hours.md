# The iPhone app goes to TestFlight every four hours, when it's changed

**Status:** Decided.
**First recorded:** 2026-11-21, in #PR.

CI uploads the iPhone app from `main` to TestFlight, for the team's own testers, every four hours if what the app is built from has changed since the last upload, and whenever someone runs the workflow by hand. Its version is the desktop's for the same commit, `0.1.<commits on main>`, as [publish-every-merge.md](publish-every-merge.md) says, so a phone build and a desktop build from one commit say the same. Its build number is the minute it was made.

## Chosen over

- **Every merge, as the desktop.** About half of the merges change what the phone builds, which on a busy day is dozens, and App Store Connect refuses uploads after some number in a day. Apple doesn't publish the number.
- **Once a day.** A fix would wait up to a day to reach a phone.
- **Only by hand.** Testers would get nothing new unless someone remembered.

## What it costs

- **A build can be up to four hours behind `main`,** unless the workflow is run by hand.
- **It goes to the team's own testers only.** Testers outside the team, and the App Store, need Apple's review first.
