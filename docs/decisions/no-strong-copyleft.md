# Nothing that ships is under GPL, AGPL or LGPL

**Status:** Decided.
**First recorded:** 2026-11-08, in #164. Using GPUI's Apache-2.0 crates and not Zed's GPL-3.0 ones is from 2026-11-06, in #78.

Everything that ships is under a permissive license, such as Apache-2.0 or MIT. MPL-2.0 is allowed too, but only in a crate that comes unchanged from crates.io. `tools/policy` checks what ships on each platform.

- **GPL and AGPL.** Code under either would put the whole app under its terms. That would decide the project's own license, and rule out a closed version or a hosted one. GPL is also widely seen as incompatible with the App Store, and AGPL's terms would reach the hub over the network.
- **LGPL.** It expects a user to be able to relink the program, which static linking makes awkward.
- **MPL-2.0.** It covers only its own files (§3.3). Shipping those files unchanged only means saying where their source is (§3.2), and that goes in the notices every license needs anyway. A changed copy would have to be published, so MPL-2.0 code comes only from crates.io.

The one exception is the AppImage runtime, as [linux-appimage.md](linux-appimage.md) says.

## Chosen over

No alternative was recorded.

## What it costs

Zed's GPL-3.0 crates are out, and only GPUI's own crates, which are under Apache-2.0, are in.
