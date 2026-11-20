# Dependencies

What a dependency has to meet before it's added: its license, and how its updates arrive. `docs/standards/rust.md` says how a crate's version is written and pinned.

- **Check primary sources for licenses.** Read `Cargo.toml` and the license terms, not GitHub's label, which reported `gpui-mobile` as AGPL when it's tri-licensed with Apache-2.0.
- **Keep strong copyleft out of shipped code.** No GPL, AGPL or LGPL, so Zed's GPL-3.0 crates are out and only GPUI's Apache-2.0 crates are in. `docs/architecture.md` has why, and the one exception, and `tools/policy` checks it.
- **Give Renovate every new dependency.** A crate goes in `[workspace.dependencies]` at its full version, as `docs/standards/rust.md` says. A version pinned anywhere else, such as a tool a workflow installs, gets a `# renovate:` comment on the line above it. If Renovate can't read it, or can't finish its update alone, as with a checksum, add a custom manager or a rule to `.github/renovate.json5` in the same PR.
