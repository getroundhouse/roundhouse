# Workarounds

The kinds of workaround, and what one needs before it's kept. A review looks for them, as `docs/standards/reviews.md` says.

**Leave the code the way you'd want to find it.** Agents copy what they find here, so a workaround, of the kinds below, becomes the pattern the next change follows. Restructure what's in the way rather than working around it, in a PR of its own at the bottom of a stack when it stands alone, and give any workaround you keep the reason this asks for. Don't copy a workaround you find: fix it when it's part of the change, and otherwise fix it in a PR of its own or file it, as "When to file" in `docs/standards/issues.md` says.

Each of these needs a reason: in a comment beside it, where `docs/standards/documentation.md` puts why code is the way it is, so the next agent sees it, and in a paragraph of its own in part 3 of the PR, as `docs/standards/pull-requests.md` says of any choice a reviewer might question. Without one, the change is restructured instead.

- A flag, mode or option added so that one caller works.
- A special case for one value, kind or caller.
- An error swallowed, or a fallback that hides a failure.
- A check for a state the code can't reach. If it can reach it, fix what lets it. A state that comes from outside the process can always arrive, such as from the network, storage, another program, or an older hub or client, and handling it is right.
- Logic copied, or nearly copied, rather than shared.
- A wrapper around code this repo owns, rather than a change to it. The copy of gpui-kit doesn't count, since what only this app needs is built in the app, as `docs/standards/gpui.md` says.
- Code that exists only for the middle of a stack, as `docs/standards/stacks.md` says.
- A sleep in the product's code, a retry of what failed, or a longer timeout, added so that a test passes. A fixed sleep in a test isn't let in even with a reason, as "Writing tests" in `docs/standards/testing.md` says.

A workaround copied from code nearby isn't let in, even with a reason. The change doesn't copy it, and the one it would copy is dealt with as above.

Working around the kit has a rule of its own, and signs of its own, in `docs/standards/gpui.md`: the kit is changed, or the app builds the piece itself, instead.
