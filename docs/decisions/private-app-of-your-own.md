# An app of your own is a private GitHub App, made under each account or organization it's for

**Status:** Decided.
**First recorded:** 2026-11-17, in #1612.

An app of your own is a GitHub App that the person makes, so agents act as its bot and not as the person. The hub writes the app's manifest, and the manifest makes the app private. GitHub lets a private app be installed only on the account or organization it was made under. So no one else can install the app, and each app has at most one installation, on its owner.

The person can still make the app public on GitHub later, and someone else can then install it. The hub connects only the installation on the app's owner, so no one else has agents act as the app.

## Chosen over

- **One public app, installed on every account and organization it's for.** Anyone on GitHub could install it.

## What it costs

The person makes an app under each organization it's for.
