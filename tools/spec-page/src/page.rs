//! The page: each area, rule by rule, each beside the screens that show it, and a switch that leaves only what the
//! owner hasn't confirmed, so an area can be confirmed at one sitting. It shows only what the index holds, and links
//! each image to itself at full size.
//!
//! The page's folder is as deep in the repo as `docs/spec/`, and the spec's screens are copied into it, so what the
//! spec points to, such as `screens/chat.png` or the app's icon in `../../apps/`, is found from the page as it's
//! written.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use maud::{DOCTYPE, Markup, PreEscaped, html};

use crate::index::{Area, Block, Confirmation, Index, NOT_BUILT, Screen, Section, UNCONFIRMED};

#[cfg(test)]
mod tests;

const STYLE: &str = include_str!("page.css");

/// From the page to the spec's files, which it links to.
const TO_THE_SPEC: &str = "../../docs/spec/";

/// Writes the page into `out`, with the index it's drawn from as `index.json` and a copy of the screens in `spec`,
/// and gives the page's path.
pub(crate) fn write(index: &Index, spec: &Path, out: &Path) -> Result<PathBuf> {
    let screens = out.join("screens");
    // So a screen the spec has dropped goes too.
    match std::fs::remove_dir_all(&screens) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
    .with_context(|| format!("clearing {}", screens.display()))?;
    std::fs::create_dir_all(&screens).with_context(|| format!("making {}", screens.display()))?;
    let from = spec.join("screens");
    for entry in std::fs::read_dir(&from).with_context(|| format!("reading {}", from.display()))? {
        let entry = entry?;
        let to = screens.join(entry.file_name());
        std::fs::copy(entry.path(), &to).with_context(|| format!("copying to {}", to.display()))?;
    }
    let write = |name: &str, contents: String| {
        let path = out.join(name);
        std::fs::write(&path, contents).with_context(|| format!("writing {}", path.display()))?;
        anyhow::Ok(path)
    };
    write("index.json", serde_json::to_string_pretty(index)?)?;
    let page = write("index.html", draw(index).into_string())?;
    Ok(page.canonicalize()?)
}

pub(crate) fn draw(index: &Index) -> Markup {
    let (all, confirmed) = index
        .areas
        .iter()
        .map(counts)
        .fold((0, 0), |(all, confirmed), (screens, of_them)| (all + screens, confirmed + of_them));
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "The spec" }
                style { (PreEscaped(STYLE)) }
            }
            body {
                // The areas are in the header, so they stay at the top as the page scrolls.
                header {
                    div.title {
                        h1 { "The spec" }
                        p { (confirmed) " of " (all) " screens confirmed" }
                        label { input type="checkbox" id="only-unconfirmed"; " Only what isn't confirmed" }
                    }
                    nav {
                        ul {
                            @for area in &index.areas {
                                @let (screens, confirmed) = counts(area);
                                li { a href={ "#" (area.file) } { (area.title) } " " span { (confirmed) " of " (screens) } }
                            }
                        }
                    }
                }
                main {
                    @for area in &index.areas { (draw_area(area)) }
                }
            }
        }
    }
}

fn draw_area(area: &Area) -> Markup {
    let (screens, confirmed) = counts(area);
    let blocks = || area.sections.iter().flat_map(|section| &section.blocks);
    let unconfirmed_rules: usize = blocks().map(|block| block.unconfirmed).sum();
    let unpictured = blocks().filter(|block| block.lead.is_some() && block.screens.is_empty()).count();
    html! {
        section.area id=(area.file) {
            h2 { (area.title) }
            p.about { (area.covers.trim_end_matches('.')) ". " a href={ (TO_THE_SPEC) (area.file) } { (area.file) } }
            p.counts {
                (confirmed) " of " (screens) " screens confirmed, " (unconfirmed_rules) " rules unconfirmed, "
                (unpictured) " rules with no picture"
            }
            @for section in &area.sections { (draw_section(section)) }
        }
    }
}

fn draw_section(section: &Section) -> Markup {
    html! {
        div.part {
            @if let Some(title) = &section.title { h3 { (title) } }
            @for block in &section.blocks { (draw_block(block)) }
        }
    }
}

/// A row: a rule, or whatever else the spec says, beside the screens that show it. A rule shows the words it starts
/// with in bold, and the rest of it a click away. Screens on their own have their caption where a rule's words go.
fn draw_block(block: &Block) -> Markup {
    let to_confirm =
        block.unconfirmed > 0 || block.screens.iter().any(|screen| screen.confirmation != Confirmation::Confirmed);
    let on_their_own = block.html.is_empty();
    let caption = block.screens.first().and_then(|screen| screen.caption.as_deref()).filter(|_| on_their_own);
    html! {
        div.row.to-confirm[to_confirm] {
            div.words {
                @if let Some(lead) = &block.lead {
                    details {
                        summary {
                            span.lead { (PreEscaped(lead)) }
                            @if block.unconfirmed > 0 { " " em.mark.unconfirmed { "Unconfirmed" } }
                            @if block.not_built { " " em.mark.not-built { "Not built" } }
                            @if block.screens.is_empty() { " " span.no-picture { "No picture" } }
                        }
                        div.rest { (marked(&block.html)) }
                    }
                } @else if let Some(caption) = caption {
                    (marked(caption))
                } @else {
                    (marked(&block.html))
                }
            }
            div.pictures {
                @for screen in &block.screens { (draw_screen(screen, !on_their_own)) }
            }
        }
    }
}

/// A screen, with its status, its name and, if `captioned`, its caption.
fn draw_screen(screen: &Screen, captioned: bool) -> Markup {
    let (class, status) = match screen.confirmation {
        Confirmation::Confirmed => ("confirmed", "Confirmed"),
        Confirmation::Unconfirmed => ("unconfirmed", "Unconfirmed"),
        Confirmation::NoCaption => ("no-caption", "No caption"),
    };
    // At the size it has in the app, since the gallery draws at twice that, and known before it loads, so the page
    // doesn't move under a link to an area while the images above it load.
    let half = screen.size.map(|size| (size.width / 2, size.height / 2));
    html! {
        figure.screen.(class) id={ "screen-" (screen.name.as_str()) } {
            a href=(screen.image) {
                img src=(screen.image)
                    width=[half.map(|(width, _)| width)]
                    height=[half.map(|(_, height)| height)]
                    alt=(screen.alt)
                    loading="lazy";
            }
            figcaption {
                span.status { (status) } " " code { (screen.name.as_str()) }
                @if let Some(caption) = screen.caption.as_ref().filter(|_| captioned) { " " (marked(caption)) }
            }
        }
    }
}

/// How many screens the area has, and how many of them are confirmed.
fn counts(area: &Area) -> (usize, usize) {
    area.screens().fold((0, 0), |(all, confirmed), screen| {
        (all + 1, confirmed + usize::from(screen.confirmation == Confirmation::Confirmed))
    })
}

/// HTML the spec's Markdown was drawn as, which is the spec's own and so isn't escaped again, with its marks picked
/// out so the stylesheet can colour them.
fn marked(html: &str) -> PreEscaped<String> {
    let marked = html
        .replace(UNCONFIRMED, "<em class=\"mark unconfirmed\">Unconfirmed")
        .replace(NOT_BUILT, "<em class=\"mark not-built\">Not built");
    PreEscaped(marked)
}
