# gpui-kit and three parts of GPUI are built from copies in the repo

**Status:** Decided.
**First recorded:** 2026-11-15, in #1499, for gpui-kit. GPUI's exact pin is from 2026-11-06, in #78. The copies of GPUI's parts were added on 2026-11-20, in #1991 and #1992.

The app builds Longbridge's gpui-kit from a copy in `third_party/gpui-kit/vendor`, at a pinned release. So a change the app needs in the kit goes in with the app code that needs it, is checked with the app, and is offered upstream from `third_party/gpui-kit/patches`.

Three more copies are in `third_party/`, at the release GPUI is pinned at: GPUI's iOS backend, its text system for Apple's systems, and the fork of font-kit that the text system uses. Each has the few changes the apps need. Each change is marked where it's made and offered upstream, so a copy can go once a release includes its changes.

The rest of GPUI comes unchanged from crates.io, pinned to one exact version, and a change it needs goes to Zed. GPUI and the kit are both pre-1.0, so an upgrade should be deliberate and happen in one place.

## Chosen over

No alternative was recorded.
