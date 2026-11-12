# Site

The marketing site: one static page, with no build step and nothing loaded from anywhere else.

```sh
python3 -m http.server 8000 --directory site   # Then open http://127.0.0.1:8000
```

That server doesn't say how long a file may be kept, so a browser can go on using a script or the stylesheet for hours after it changes. Reload with `Cmd-Shift-R` after a change.

## What it says

- **It states a take, and says once what isn't built.** Running many agents should feel calm, and each section under the headline is a belief, stated rather than argued. Everything it says the app does is a rule in `docs/spec/` that isn't marked *Not built*, so check a claim there before adding it, and when a rule it rests on changes. More agents, the hub on a server or in the cloud, and the iPhone app are named as next, in one place each.
- **The headline's last word changes, and each word has a section behind it:** calm has "Quiet until it needs you", native has "Native. Not a web view.", under control has "Hand it over. Know where it stands.", and like home has "Coming from Conductor?". A word with nothing on the page to back it doesn't go in the headline. Calm comes first, and it's the word the page holds on when it's held still.
- **Control means handing work over predictably, not deciding each step.** The page doesn't say the person makes every call, since the point is that agents can be left to work, with full access, and that where each one stands is always plain. Plan mode is offered as the way to see the route first.
- **Why this exists is in the owner's voice,** so the owner checks what it claims.
- **It names Conductor as the README does:** as what showed how good this can feel, and as what a repository may already be set up for. It doesn't call Roundhouse a clone of it or a replacement for it.
- **The small things are ones a person feels,** such as a prompt that waits for setup, rather than what every tool of this kind has, such as worktrees.
- **It says it's open source once, in a statement of its own, and links to the repo from four places:** the nav, that statement, beside the last Download, and the footer. The link is the word GitHub rather than GitHub's mark, so the page shows nobody else's logo. On a phone under 350 pixels wide the nav has no room for it, and leaves it to the other three.
- **The hero has one download, the visitor's own:** the Mac's, or on Windows the installer for x64, or for ARM64 where the browser says the processor is ARM. Anyone else, such as someone on a phone or on Linux, is offered the Mac's. The nav's Download is the same one. Each is the newest build, by the name `tools/package/publish.py` gives the one people download, beside its update feed.
- **Every download is in one place, the closing section,** a card for each platform with the visitor's own in copper, so the hero keeps one action. The line beside the hero's button ends in All downloads, which goes there.
- **The Mac is the platform, and Windows is a first preview, and the page says so wherever Windows is offered:** each Windows card says Preview, the line under the cards says what that means, that it's unsigned, so Windows asks before it runs, and the hero says it to someone on Windows. The headline, the closing section and the footer still say Mac. The Mac's is for Apple silicon, and the page says that too.
- **What changes with the platform is in the page once for each, and a rule in the stylesheet shows one,** by what the page's first script puts on `<html>`, before anything is drawn. So the button never changes under someone, and without scripts the page offers the Mac's. Only whether Windows is on ARM comes later, from `site.js`, since a browser says that only when asked.

## How it follows the app's look

- **It looks like the app.** Its colours and type come from `docs/spec/look.md` and `crates/look/src/theme.json`, and its pictures are the app's own screens from `docs/spec/screens/`, as WebP in `assets/`: `conversation.png` whole, with the window's controls added on the page, the approval's card from `approval.png`, and five rows of `pull-request-states.png`. Take them again when those screens change.
- **`fan.js` draws the status mark the way `crates/look` does,** with the same geometry, place in its box, colours and motion, so a change to the mark in the app changes it here too. A fan wider than the app's largest gets a row for every 36 pixels or so, so its dots stay fine. The hero's light starts and ends off the fan, so it doesn't blink out of the outer row as it comes round, and it's over the middle of the fan when the page opens, so the headline's first word stays as long as the others.
- **Copper is treated as a material, not only a colour.** The headline's changing word is copper with a sheen that crosses it as the fan's light crosses the fan. The main button is brushed copper. Boxes have metal edges that catch the light along the top, the hub's box is turned metal, and the rules warm to copper in the middle. Behind the hero, the hub and the closing mark, a halftone glow radiates from each fan, and the sections of text between them stay clean.
- **A link among words is underlined in copper,** faint until the pointer is on it, so it reads as a link without standing out of its sentence. A link that stands alone, as in the nav, is muted and brightens.
- **Only an agent with something running moves a mark,** as in the app, and a workspace that needs a look has a brighter name rather than a different mark. The system's Reduce Motion setting holds every mark and the headline still, with the hero's light where the app icon has it.
- **Nothing sharp slides.** The headline's second line is centred for its first word, and the other words run on from where that one starts, so the words before them stay put. On a phone the changing word has a line of its own.

## How it's published

- **GitHub Pages serves it from `main`.** `.github/workflows/site.yml` publishes `site/` whenever it changes, put together by `tools/site/assemble.py`, which leaves this file out.
- **Its address is `https://getroundhouse.dev`,** the custom domain in the repo's Pages settings. The page and the scripts don't name it: the deploy reads it from the `SITE_URL` secret, to add the link-preview tags that need it, so the site can move without a change here. `tools/site/assemble.py OUT https://example.com` shows the page as it's published.
- **The link preview's picture, `assets/card.png`, is the hero held still,** without the nav and the button, at 1200 by 630. Take it again when the hero changes.
