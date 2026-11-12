# Inspiration

What we've learned from Conductor, the app that inspired this one, from its docs, screenshots of it and its installed app. It's a source of ideas, not a spec, and no code or assets are copied. New users can no longer sign up or download it, so the installed copy is the only way left to try it.

## Product model

From Conductor's docs (`/docs/concepts/workflow`):

- One shippable unit maps to one workspace, one branch, and typically one PR.
- A workspace has its own branch, working tree, files, app processes and agent context.
- The lifecycle is: break down the work, create workspaces, run agents independently, verify and review and resolve, then open a PR, merge and archive.
- Durable guidance lives in repository settings or instruction files. Task context comes from chat, attachments and `.context`.
- A project is Conductor's entry for a codebase, holding its settings, scripts, instructions and workspaces, with one git repository behind it (`/docs/concepts/workspaces-and-branches`). The interface says "project", and keeps "repository" for the git codebase itself, as in "Repository Settings".
- Its sidebar has an "Add project" button. Release 0.70.0 moved it into the dialog for a new workspace, beside the repository picker, and 0.70.2 put it back in the sidebar and added it to the command palette.

## Layout

Measured from screenshots of its chat, on a Retina display:

- The composer is centred under the chat, at most 896px wide, and about 152px tall when it's empty. Its send and stop button is 24px square, about 14px from its right edge and 16px from its bottom edge.
- The chat starts 24 to 26px inside the composer's left edge. Where the chat's column ends on the right didn't show, since no line ran that far.
- A prompt is a bubble at the right of the chat, only as wide as its text, with no edge and small corners, about 6px. A one-line prompt is about 44px tall, with about 16px of room beside its text.
- A prompt has more room above it than below, about 40px against 20px, so each prompt starts a new stretch of the conversation.
- In a screenshot of a window 1728px wide, the sidebar on the left is 290px wide and the one of changes on the right 432.5px, each counting its 1px edge against the chat.

## How text looks

From the stylesheets in the installed app, which is a web view:

- **Font smoothing is off for every piece of text,** with `-webkit-font-smoothing: antialiased` on the page. So text is drawn the same weight whatever its colour. With smoothing on, macOS thickens brighter text more, so a name that turns white looks heavier.
- **Selecting a sidebar row changes only its background.** A name with nothing unread stays muted, white at 60%, whether or not its row is selected. Only a name with an unread reply turns brighter, white at 90%, and heavier, in medium or semibold, which is a setting.
- **The font is the system's, SF Pro, and a name with nothing unread is at weight 450,** between regular and medium.

## Finding a prompt

From screenshots of a long chat, on a Retina display:

- At the top right of the chat, just left of its scrollbar, is a short tick for each prompt, about 20px wide and 8px apart, in grey, with the one in view in white. It reads as a map of the chat's prompts, and of where you are among them.
- Resting the pointer on the ticks opens a raised list over them, which hides them while it's open, so the ticks themselves are never clicked. It's about 255px wide, with one row per prompt, about 26px tall, each cut short with an ellipsis. The prompt in view has a lighter row, and the list scrolls, with its own scrollbar, once there are more than about a dozen.
- A chat scrolled up from its end has a button back down at its bottom left: a 28px square with corners of about 6px, on the pane's colour, with an edge of about `#434140` and a small chevron pointing down, nearly white. Its left edge is on the composer's, so it reaches about 3px over the chat's left edge, and it sits about 9px above the composer.

## Settings' sidebar

From a screenshot of the top of its Settings sidebar, on a Retina display, in points:

- **A search sits at the top, saying "Search settings" and showing `⌘ F` at its right end.** It's 28 tall, with corners of about 6, on a darker fill than the sidebar, the pane's colour, and an edge lighter than the sidebar's own, about `#434140`. Its magnifying glass is muted, about 14 wide, 10 in from the edge, and what it says is fainter than muted text, about 9 after the glass.
- **The key is a small raised key,** 16 tall and 26 wide, with corners of about 4 and an edge of its own, 8 from the field's right edge. The `⌘` is smaller than the `F`, and the two are about 5 apart, both muted.
- **Pages are under a heading, Personal, in muted text at about 12,** not bold, 12 below the search, on a line 28 tall, set in as far as a row's icon.
- **A page's row is 32 tall, with an icon of two sliders, 16 wide and 8 in, then its name at body size, 12 after the icon,** both in the text colour. The row showing is filled a little lighter than the sidebar, with a control's corners. It reaches about 2 further left than the search, and ends level with it on the right.

## Sources

Conductor's docs run to 68 pages, listed at `conductor.build/llms.txt`. A private reference copy and all 209 changelog entries are on the owner's Mac, in `~/conductor/archived-contexts/conductor-clone/merged-state/conductor-docs/`. They're the best source of ideas for new features. Don't republish them.

Where the docs and the changelog disagree, the changelog is newer. For example, the FAQ says the sidebar shows a workspace's PR title or branch, but later releases name a workspace from its first prompt and show that name.

- Conductor: [announcement](https://www.conductor.build/announcement), [changelog](https://conductor.build/changelog), [workflow](https://conductor.build/docs/concepts/workflow), [API](https://www.conductor.build/docs/api), [cloud FAQ](https://www.conductor.build/docs/cloud/faq), [docs index](https://conductor.build/llms.txt)
