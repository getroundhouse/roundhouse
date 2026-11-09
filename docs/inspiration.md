# Inspiration

What we've learned from Conductor, the app that inspired this one, from its docs and from screenshots of it. It's a source of ideas, not a spec, and no code or assets are copied. New users can no longer sign up or download it, so the installed copy is the only way left to try it.

## Product model

From Conductor's docs (`/docs/concepts/workflow`):

- One shippable unit maps to one workspace, one branch, and typically one PR.
- A workspace has its own branch, working tree, files, app processes and agent context.
- The lifecycle is: break down the work, create workspaces, run agents independently, verify and review and resolve, then open a PR, merge and archive.
- Durable guidance lives in repository settings or instruction files. Task context comes from chat, attachments and `.context`.

## Layout

Measured from screenshots of its chat, on a Retina display:

- The composer is centred under the chat, at most 896px wide, and about 152px tall when it's empty. Its send and stop button is 24px square, about 14px from its right edge and 16px from its bottom edge.
- The chat starts 24 to 26px inside the composer's left edge. Where the chat's column ends on the right didn't show, since no line ran that far.
- A prompt is a bubble at the right of the chat, only as wide as its text, with no edge and small corners, about 6px. A one-line prompt is about 44px tall, with about 16px of room beside its text.
- A prompt has more room above it than below, about 40px against 20px, so each prompt starts a new stretch of the conversation.

## Sources

Conductor's docs run to 68 pages, listed at `conductor.build/llms.txt`. A private reference copy and all 209 changelog entries are on the owner's Mac, in `~/conductor/archived-contexts/conductor-clone/merged-state/conductor-docs/`. They're the best source of ideas for new features. Don't republish them.

Where the docs and the changelog disagree, the changelog is newer. For example, the FAQ says the sidebar shows a workspace's PR title or branch, but later releases name a workspace from its first prompt and show that name.

- Conductor: [announcement](https://www.conductor.build/announcement), [changelog](https://conductor.build/changelog), [workflow](https://conductor.build/docs/concepts/workflow), [API](https://www.conductor.build/docs/api), [cloud FAQ](https://www.conductor.build/docs/cloud/faq), [docs index](https://conductor.build/llms.txt)
