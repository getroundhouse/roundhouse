use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use maud::{DOCTYPE, Markup, PreEscaped, html};

use crate::index::{Area, Block, Index, Place, Screen, Section, Table};
use crate::page::marked;

pub(super) const STYLE: &str = include_str!("../design.css");
const SCRIPT: &str = include_str!("../design.js");

/// The spec's file that names the foundations.
pub(crate) const FOUNDATIONS: &str = "look.md";

/// The spec's file that has the components built on the foundations.
pub(crate) const COMPONENTS: &str = "components.md";

/// The type whose table names the colours, which the page is in.
pub(crate) const COLOURS: &str = "look::Colors";

/// Writes the page into `out`, beside the spec's page, whose copy of the screens it shows, and gives its path.
pub(crate) fn write(index: &Index, out: &Path) -> Result<PathBuf> {
    let file = |name: &str| {
        let area = index.areas.iter().find(|area| area.file == name);
        area.with_context(|| format!("the spec has no {name}"))
    };
    let page = draw(file(FOUNDATIONS)?, file(COMPONENTS)?).into_string();
    let path = out.join("design.html");
    std::fs::write(&path, page).with_context(|| format!("writing {}", path.display()))?;
    Ok(path.canonicalize()?)
}

/// The page: the foundations, then the components built on them, each under a heading of its own.
pub(crate) fn draw(foundations: &Area, components: &Area) -> Markup {
    let halves = [Half { title: "Foundations", area: foundations }, Half { title: "Components", area: components }];
    let style = format!("{}\n{STYLE}", colours(foundations));
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "The design system" }
                style { (PreEscaped(style)) }
            }
            body {
                // Stays beside the page as it scrolls, with the box that finds a name.
                aside {
                    h1 { "The design system" }
                    p.about {
                        "What " (spec_link(foundations)) " and " (spec_link(components)) " say, each name beside \
                        the app's drawing of it. " a href="index.html" { "The spec" }
                    }
                    input #find type="search" placeholder="Find a name, a value or a use" aria-label="Find a name, \
                        a value or a use" autocomplete="off";
                    nav { ul { @for half in &halves { (draw_nav(half)) } } }
                }
                main {
                    p #nothing hidden { "No name, value or use has that in it." }
                    @for half in &halves {
                        section.half id=(slug(half.title)) {
                            h2 { (half.title) }
                            @for section in &half.area.sections { (draw_section(half, section)) }
                        }
                    }
                }
                script { (PreEscaped(SCRIPT)) }
            }
        }
    }
}

/// The foundations or the components, under its own heading.
struct Half<'a> {
    title: &'a str,
    area: &'a Area,
}

impl Half<'_> {
    /// A heading's ID, which says which half it's in, since each half's file has headings of its own.
    fn anchor(&self, title: &str) -> String {
        format!("{}-{}", slug(self.title), slug(title))
    }
}

/// A link to the spec's file, from where the page is written.
fn spec_link(area: &Area) -> Markup {
    html! { a href={ "../../docs/spec/" (area.file) } { (area.file) } }
}

/// A part of what a section says.
enum Part<'a> {
    /// Its rules, together where the first of them is.
    Rules(Vec<&'a Block>),
    Named(Named<'a>),
    /// Words that aren't a rule, such as a paragraph under a table, with any screens they have.
    Words(&'a Block),
    Screen(&'a Screen),
}

/// The names a type in `look` gives, in the spec's table of them, under the caption that names the type.
pub(super) struct Named<'a> {
    /// Such as `look::Space`.
    pub(super) kind: &'a str,
    caption: &'a Block,
    table: &'a Table,
    /// The gallery's sheet of them, which each row's picture is cut from.
    pub(super) sheet: Option<&'a Screen>,
}

impl Named<'_> {
    /// Its rows that are names, rather than the heading of a group of them.
    pub(super) fn rows(&self) -> impl Iterator<Item = &Vec<String>> {
        self.table.rows.iter().filter(|row| !is_group(row))
    }
}

/// What a section says, in its order, with its rules together where the first of them is.
fn sort<'a>(area: &'a Area, section: &'a Section) -> Vec<Part<'a>> {
    let mut parts = Vec::new();
    let mut rules = Vec::new();
    let mut rules_at = None;
    let mut blocks = section.blocks.iter().peekable();
    while let Some(block) = blocks.next() {
        if let Some(named) = blocks.peek().and_then(|next| named(area, block, next)) {
            parts.push(Part::Named(named));
            blocks.next();
        } else if block.lead.is_some() {
            rules_at.get_or_insert(parts.len());
            rules.push(block);
        } else if !block.html.is_empty() {
            parts.push(Part::Words(block));
        } else {
            parts.extend(block.screens.iter().map(Part::Screen));
        }
    }
    if let Some(at) = rules_at {
        parts.insert(at, Part::Rules(rules));
    }
    parts
}

/// The table in `block` as the names the type its caption names gives, if the caption names one.
fn named<'a>(area: &'a Area, caption: &'a Block, block: &'a Block) -> Option<Named<'a>> {
    let table = block.table.as_ref()?;
    caption.lead.as_ref()?;
    let kind = kind(&caption.html)?;
    let sheet = area.screens().find(|screen| screen.places.contains_key(kind));
    Some(Named { kind, caption, table, sheet })
}

/// The type in `look` that a caption names first, in code, such as `look::Space`.
fn kind(caption: &str) -> Option<&str> {
    let start = caption.find("<code>look::")? + "<code>".len();
    let end = start + caption[start..].find("</code>")?;
    Some(&caption[start..end])
}

/// Every table of names in the area, with its section's title.
pub(super) fn all_named(area: &Area) -> impl Iterator<Item = (&str, Named<'_>)> {
    area.sections.iter().flat_map(move |section| {
        let title = section.title.as_deref().unwrap_or_default();
        sort(area, section).into_iter().filter_map(move |part| match part {
            Part::Named(named) => Some((title, named)),
            Part::Rules(_) | Part::Words(_) | Part::Screen(_) => None,
        })
    })
}

/// The page's colours, as CSS properties named as the spec's table of colours names them, such as `--pane`.
pub(super) fn colours(area: &Area) -> String {
    let named = all_named(area).find(|(_, named)| named.kind == COLOURS);
    let rows = named.iter().flat_map(|(_, named)| named.rows());
    let properties = rows.filter_map(|row| {
        let (name, value) = (text(row.first()?), text(row.get(1)?));
        let hex = value.strip_prefix('#').is_some_and(|hex| hex.chars().all(|c| c.is_ascii_hexdigit()));
        hex.then(|| format!("  --{name}: {value};\n"))
    });
    format!(":root {{\n{}}}", properties.collect::<String>())
}

/// The half's heading, with its headings under it, each with those under it, and how many names each has.
fn draw_nav(half: &Half) -> Markup {
    let area = half.area;
    let mut tops: Vec<(&Section, Vec<&Section>)> = Vec::new();
    for section in area.sections.iter().filter(|section| section.title.is_some()) {
        match tops.last_mut() {
            Some((top, under)) if section.depth > top.depth => under.push(section),
            _ => tops.push((section, Vec::new())),
        }
    }
    let link = |section: &Section| {
        let title = section.title.as_deref().unwrap_or_default();
        let count: usize = sort(area, section)
            .iter()
            .map(|part| match part {
                Part::Named(named) => named.rows().count(),
                Part::Rules(_) | Part::Words(_) | Part::Screen(_) => 0,
            })
            .sum();
        html! { a href={ "#" (half.anchor(title)) } { (title) @if count > 0 { " " span.count { (count) } } } }
    };
    html! {
        li.half {
            a href={ "#" (slug(half.title)) } { (half.title) }
            ul {
                @for (top, under) in &tops {
                    li {
                        (link(top))
                        @if !under.is_empty() { ul { @for section in under { li { (link(section)) } } } }
                    }
                }
            }
        }
    }
}

/// A section under its heading, in its order, with its rules folded away when it has names to look up.
/// What comes before the first heading says what the area is about, and goes at the top of its half.
fn draw_section(half: &Half, section: &Section) -> Markup {
    let area = half.area;
    let Some(title) = &section.title else {
        return html! { div.intro { @for block in &section.blocks { (marked(&block.html)) } } };
    };
    let parts = sort(area, section);
    let looked_up = parts.iter().any(|part| matches!(part, Part::Named(_)));
    html! {
        section id=(half.anchor(title)) {
            @if section.depth > 2 { h4 { (title) } } @else { h3 { (title) } }
            @for part in &parts {
                @match part {
                    Part::Rules(rules) => @if looked_up {
                        details.rules-folded { summary { "The rules, " (rules.len()) } (draw_rules(rules)) }
                    } @else {
                        (draw_rules(rules))
                    },
                    Part::Named(named) => (draw_named(title, named)),
                    Part::Words(block) => div.words {
                        (marked(&block.html))
                        @for screen in &block.screens { (draw_screen(screen)) }
                    },
                    Part::Screen(screen) => (draw_screen(screen)),
                }
            }
        }
    }
}

fn draw_rules(rules: &[&Block]) -> Markup {
    html! {
        div.rules {
            @for rule in rules { div.rule { (marked(&rule.html)) @for screen in &rule.screens { (draw_screen(screen)) } } }
        }
    }
}

/// A screen, and a sheet folded away, since its pieces are in the table above it.
fn draw_screen(screen: &Screen) -> Markup {
    let half = screen.size.map(|size| (size.width / 2, size.height / 2));
    let figure = html! {
        figure.screen {
            a href=(screen.image) {
                img src=(screen.image) width=[half.map(|(width, _)| width)] height=[half.map(|(_, height)| height)]
                    alt=(screen.alt) loading="lazy";
            }
            @if let Some(caption) = &screen.caption { figcaption { (marked(caption)) } }
        }
    };
    if screen.places.is_empty() {
        figure
    } else {
        html! { details.sheet { summary { "The whole sheet" } (figure) } }
    }
}

fn draw_named(title: &str, named: &Named) -> Markup {
    // The names with each value.
    let mut same: BTreeMap<&[String], Vec<String>> = BTreeMap::new();
    for row in named.rows().filter(|row| !value(row).is_empty()) {
        same.entry(value(row)).or_default().push(text(&row[0]));
    }
    let width = named.table.header.len() + 1;
    html! {
        div.named {
            div.caption { (marked(&named.caption.html)) }
            table {
                thead { tr { th {} @for cell in &named.table.header { th { (PreEscaped(cell)) } } } }
                tbody {
                    @for row in &named.table.rows {
                        @if is_group(row) {
                            tr.group { th colspan=(width) { (PreEscaped(&row[0])) } }
                        } @else {
                            (draw_name(title, named, row, same.get(value(row)).map_or(&[][..], Vec::as_slice)))
                        }
                    }
                }
            }
        }
    }
}

/// A name's row: its piece of the sheet, the name, its value and what it's for, and the other names with the same
/// value, out of `same`, which has it too.
fn draw_name(title: &str, named: &Named, row: &[String], same: &[String]) -> Markup {
    let name = text(&row[0]);
    let anchor = format!("{}-{name}", named.kind.trim_start_matches("look::"));
    let place = named.sheet.and_then(|sheet| Some((sheet, *sheet.places.get(named.kind)?.get(&name)?)));
    let words = [title, named.kind].into_iter().map(str::to_string).chain(row.iter().map(|cell| text(cell)));
    let find = words.collect::<Vec<_>>().join(" ").to_lowercase();
    let others: Vec<&String> = same.iter().filter(|other| **other != name).collect();
    html! {
        tr.name id=(anchor) data-find=(find) {
            td.picture {
                @if let Some((sheet, place)) = place { (cut(sheet, place, &name)) } @else { span.no-picture { "No picture" } }
            }
            td.name { a href={ "#" (anchor) } { (PreEscaped(&row[0])) } }
            @for cell in value(row) { td.value { (PreEscaped(cell)) } }
            @if row.len() > 1 {
                td.for {
                    (marked(&row[row.len() - 1]))
                    @if !others.is_empty() {
                        div.same { "Same as " @for (ix, other) in others.iter().enumerate() { @if ix > 0 { ", " } code { (other) } } }
                    }
                }
            }
        }
    }
}

/// The piece of `sheet` at `place`, at the size it has in the app, since the gallery draws at twice that.
fn cut(sheet: &Screen, place: Place, name: &str) -> Markup {
    let Some(size) = sheet.size else { return html! { span.no-picture { "No picture" } } };
    let half = |pixels: u32| f64::from(pixels) / 2.;
    let style = format!(
        "width: {}px; height: {}px; background-image: url('{}'); background-size: {}px {}px; \
         background-position: -{}px -{}px",
        half(place.width),
        half(place.height),
        sheet.image,
        half(size.width),
        half(size.height),
        half(place.x),
        half(place.y),
    );
    html! { span.cut role="img" aria-label=(name) style=(style) {} }
}

/// A row that heads a group of names, such as the colours' Surfaces: words in bold, and nothing else.
fn is_group(row: &[String]) -> bool {
    row.first().is_some_and(|name| name.starts_with("<strong>")) && row.iter().skip(1).all(String::is_empty)
}

/// What a name stands for, such as a colour or a size: its row's cells between the name and what it's for.
fn value(row: &[String]) -> &[String] {
    if row.len() > 2 { &row[1..row.len() - 1] } else { &[] }
}

/// What HTML says, without its tags, and with what it had to escape put back.
pub(super) fn text(html: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            c if !in_tag => text.push(c),
            _ => {}
        }
    }
    text.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&amp;", "&")
}

/// A heading as an ID to link to, such as `space-and-shape`.
fn slug(title: &str) -> String {
    let words = title.split(|c: char| !c.is_alphanumeric()).filter(|word| !word.is_empty());
    words.map(str::to_lowercase).collect::<Vec<_>>().join("-")
}
