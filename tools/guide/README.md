# The guide

How the guide in `docs/guide` is organized and written, and how it's built into the site's `/docs/` pages.

```sh
rm -rf .context/site
tools/site/assemble.py .context/site && cargo run -p guide -- .context/site/docs
python3 -m http.server 8000 --directory .context/site   # Then open http://127.0.0.1:8000/docs/
cargo test -p guide               # The tool, against made-up guides
```

The guide is for someone using Roundhouse. What the product does and why is in `docs/spec/`, which stays the source: the guide decides nothing about the app.

## How it's organized

- **Two layers do two jobs.** A guide is named for what someone gets done, such as "Steer or queue while it works", so its name says its value, and the guides run in the order the work goes. A reference page is named for a thing, such as Chats, and what's part of a thing is a page under it, so a question's keys are under Chats rather than a feature of their own. A place in the window, such as the sidebar, is neither: it's where things show, and `concepts/the-window.md` names each once.
- **Get started opens with the quickstart,** the one path through everything, from a download to a merged pull request, on one page. It's first because someone new wants the app working before they want it explained. `get-started/what-it-does.md` is the inventory: everything the app does, each with where to read more.
- **Concepts hold what the other pages assume:** what a workspace owns, what the hub is, and what full access means.
- **The roadmap says direction, not schedule:** what's built on each platform, and what's designed and not built. Order and progress are on GitHub, as `docs/documentation.md` says, so the page links to the milestones and has no dates.
- **`README.md` holds the guide's contents, and is the page the guide opens on.** Each `## ` heading is a section of the navigation, and each item of the list under it starts with a link to a page, with the pages under that page in a list of the item's own. Every other page is in it once.

## How a page is written

- **Everything a page says the app does is a rule in `docs/spec/` that isn't marked *Not built*.** Check a claim there before adding it, and change the page in the PR that changes the rule it rests on. A rule's *Unconfirmed* mark doesn't matter here, since it's about who approved the rule, not whether it's built. What the spec doesn't cover comes from the README and the code: what installing needs, repository settings, the command line, and where data is kept.
- **What isn't built is said in one place, the roadmap.** No other page mentions it, but where someone would look for it, such as a difference on Windows.
- **It's written to the person using the app,** as "you", in the words the app shows, which `docs/terms.md` lists: a chat, not a session, and a project where the app says one. Keys are written as the README writes them, such as `Cmd-N`.
- **A page starts with its title and one paragraph saying what it's for,** which the site sets apart, and which a search engine shows.
- **A picture is one of the app's own screens, from `docs/spec/screens/`,** so a test already fails when it no longer matches the app. A page never has a picture made for it.
- **A link to another page is its path from this one, ending in `.md`,** so it works where the guide is read on GitHub too. A link to a file elsewhere in the repository is published as a link to it on GitHub.
- **A page is plain Markdown:** one title, then headings, lists, tables, code, pictures, and GitHub's notes, such as `> [!WARNING]` on a line of its own. HTML, footnotes, boxes to tick, a link in a heading and links named apart from where they're used are refused, so a page reads the same in both places.

## What building it checks

The tool stops, and writes nothing, when:

- A link points to a page, a file or a picture that isn't there, or to a heading its page doesn't have, or its address is neither a path nor on the web.
- A page isn't in the contents, or is in them twice, or the contents list something that isn't another page.
- A page doesn't start with its title, has a second one, or uses something the pages aren't drawn with.

`cargo test` doesn't build this repository's own guide. CI skips its checks for a PR that changes only Markdown, so a test of the guide's pages would fail later, in a PR that didn't break them. CI's `guide` job builds it instead, in `.github/workflows/ci.yml`, for each PR that changes what the guide is made of: its pages, the screens and the README they show and link to, the tool, and what the tool is built with. A page can link to any file in the repository, so a PR that takes any file away, or renames one, builds it too. The merge queue waits for it, as it does for the rest of CI. Run the build here after changing a page, to find out sooner.

## How it's published

- **Each page is a directory,** so `reference/chats.md` is `/docs/reference/chats/`, and the pages under it are inside it. Every link a page makes is relative, so the site can be served from any address.
- **The pages are drawn into `src/page.html`,** which has the site's header and footer written again, since `site/index.html` has no build step to share them through, so a change to either goes in both, and styled by `site/docs.css` over `site/style.css`, so the guide and the page before it are one site. `site/README.md` has the look they follow.
- **Pictures are copied to `/docs/images/`, each at its path in the repository,** only the ones a page shows, and drawn at half their pixels, since the gallery draws the app at two pixels to a point. A picture links to itself, so one drawn smaller to fit can be opened whole.
- **There's one version, `main`'s,** since the app updates itself from every merge. `.github/workflows/site.yml` builds the guide into the site, and publishes again whenever the guide, its pictures or the tool change.
