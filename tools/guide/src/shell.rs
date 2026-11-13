//! What's around a page's own words: the site's header, the guide's navigation, the headings on the page, and the
//! pages before and after it.

use crate::contents::{Contents, Entry};
use crate::draw::escape;
use crate::page::{Page, PagePath};

/// What the navigation calls the page the guide opens on.
pub(crate) const FIRST_LABEL: &str = "Overview";

/// A page in the order the guide is read in, by what the navigation calls it.
#[derive(Clone, Copy)]
pub(crate) struct Placed<'a> {
    pub(crate) page: &'a Page,
    pub(crate) label: &'a str,
}

/// The whole page, with `content` as its own words, and links to the pages read `before` and `after` it. It's drawn
/// into `page.html`, which names each place as `{name}`. `format!` fills them, so a place with nothing to fill it, or
/// the reverse, doesn't compile, and nothing a value says is taken for a place.
pub(crate) fn around(
    page: &Page,
    contents: &Contents,
    before: Option<Placed<'_>>,
    after: Option<Placed<'_>>,
    content: &str,
) -> String {
    let docs = page.path.up();
    format!(
        include_str!("page.html"),
        title = escape(&page.title),
        summary = escape(&page.summary),
        site = format_args!("{docs}../"),
        docs = docs,
        nav = nav(&page.path, contents),
        headings = on_this_page(page),
        pager = pager(&page.path, before, after),
        edit = escape(&page.path.file().on_github()),
        content = content,
    )
}

/// The contents as links, with the page showing marked.
fn nav(from: &PagePath, contents: &Contents) -> String {
    let sections: Vec<String> = contents
        .sections
        .iter()
        .map(|section| format!("<h2>{}</h2>\n{}", escape(&section.title), list(from, &section.entries)))
        .collect();
    let first = link(from, &PagePath::contents(), FIRST_LABEL);
    format!(
        "<nav class=\"guide-nav\" aria-label=\"Guide\">\n<ul>\n<li>{first}</li>\n</ul>\n{}</nav>",
        sections.concat()
    )
}

fn list(from: &PagePath, entries: &[Entry]) -> String {
    let item = |entry: &Entry| {
        let under = if entry.under.is_empty() { String::new() } else { list(from, &entry.under) };
        format!("<li>{}{under}</li>\n", link(from, &entry.page, &entry.label))
    };
    format!("<ul>\n{}</ul>\n", entries.iter().map(item).collect::<Vec<_>>().concat())
}

fn link(from: &PagePath, to: &PagePath, label: &str) -> String {
    let current = if from == to { " aria-current=\"page\"" } else { "" };
    format!("<a href=\"{}\"{current}>{}</a>", from.href(to), escape(label))
}

/// The page's `## ` headings, for a page with enough of them to be worth a list.
fn on_this_page(page: &Page) -> String {
    let sections: Vec<_> = page.headings.iter().filter(|heading| heading.depth == 2).collect();
    if sections.len() < 2 {
        return String::new();
    }
    let items: Vec<String> = sections
        .iter()
        .map(|heading| {
            format!("<li><a href=\"#{}\">{}</a></li>\n", escape(heading.slug.as_str()), escape(&heading.text))
        })
        .collect();
    format!(
        "<aside class=\"guide-headings\" aria-label=\"On this page\">\n<p>On this page</p>\n<ul>\n{}</ul>\n</aside>",
        items.concat()
    )
}

/// Links to the pages read before and after this one.
fn pager(from: &PagePath, before: Option<Placed<'_>>, after: Option<Placed<'_>>) -> String {
    let step = |to: Option<Placed<'_>>, class: &str, says: &str| match to {
        Some(Placed { page, label }) => {
            format!(
                "<a class=\"{class}\" href=\"{}\"><span>{says}</span>{}</a>\n",
                from.href(&page.path),
                escape(label)
            )
        }
        None => String::new(),
    };
    let before = step(before, "before", "Previous");
    let after = step(after, "after", "Next");
    format!("<nav class=\"guide-pager\" aria-label=\"Pages\">\n{before}{after}</nav>")
}
