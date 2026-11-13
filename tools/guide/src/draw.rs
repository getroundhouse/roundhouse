//! Draws a page's Markdown as HTML.

use std::collections::BTreeMap;

use markdown::mdast::{AlignKind, Blockquote, Code, Heading, Image, Link, List, ListItem, Node, Paragraph, Table};

use crate::error::{Error, NotDrawn};
use crate::links::{Target, target};
use crate::page::{Page, PagePath, plain_all};
use crate::picture::Pictures;
use crate::slug::Slugs;
use crate::source::Source;

mod callout;

/// The page's own words as HTML, with each picture it shows noted in `pictures`. `pages` is every page of the guide,
/// which a link to one is checked against.
pub(crate) fn draw(
    source: &Source,
    page: &Page,
    pages: &BTreeMap<PagePath, Page>,
    pictures: &mut Pictures,
) -> Result<String, Error> {
    Drawing { source, page, pages, pictures, slugs: Slugs::default() }.node(&page.root)
}

struct Drawing<'a> {
    source: &'a Source,
    page: &'a Page,
    pages: &'a BTreeMap<PagePath, Page>,
    pictures: &'a mut Pictures,
    /// The slugs of the headings drawn, made as the page's own list of its headings was, so they're the same.
    slugs: Slugs,
}

impl Drawing<'_> {
    fn all(&mut self, nodes: &[Node]) -> Result<String, Error> {
        nodes.iter().map(|node| self.node(node)).collect()
    }

    fn node(&mut self, node: &Node) -> Result<String, Error> {
        Ok(match node {
            Node::Root(root) => self.all(&root.children)?,
            Node::Paragraph(paragraph) => self.paragraph(paragraph)?,
            Node::Heading(heading) => self.heading(heading)?,
            Node::Text(text) => escape(&text.value),
            Node::Emphasis(emphasis) => format!("<em>{}</em>", self.all(&emphasis.children)?),
            Node::Strong(strong) => format!("<strong>{}</strong>", self.all(&strong.children)?),
            Node::Delete(delete) => format!("<del>{}</del>", self.all(&delete.children)?),
            Node::InlineCode(code) => format!("<code>{}</code>", escape(&code.value)),
            Node::Break(_) => "<br />\n".to_string(),
            Node::ThematicBreak(_) => "<hr />\n".to_string(),
            Node::Code(code) => code_block(code),
            Node::Blockquote(quote) => self.quote(quote)?,
            Node::List(list) => self.list(list)?,
            Node::Link(link) => self.link(link)?,
            Node::Image(image) => self.picture(image)?.1,
            Node::Table(table) => self.table(table)?,
            Node::Html(_) => return Err(self.not_drawn(NotDrawn::Html)),
            Node::LinkReference(_) | Node::ImageReference(_) | Node::Definition(_) => {
                return Err(self.not_drawn(NotDrawn::NamedApart));
            }
            Node::FootnoteDefinition(_) | Node::FootnoteReference(_) => return Err(self.not_drawn(NotDrawn::Footnote)),
            // A list draws its own items, and a table its own rows and cells. The rest are what the guide's Markdown
            // leaves off.
            Node::ListItem(_)
            | Node::TableRow(_)
            | Node::TableCell(_)
            | Node::Math(_)
            | Node::InlineMath(_)
            | Node::Toml(_)
            | Node::Yaml(_)
            | Node::MdxJsxFlowElement(_)
            | Node::MdxjsEsm(_)
            | Node::MdxTextExpression(_)
            | Node::MdxJsxTextElement(_)
            | Node::MdxFlowExpression(_) => return Err(self.not_drawn(NotDrawn::NotMade)),
        })
    }

    fn not_drawn(&self, what: NotDrawn) -> Error {
        Error::Unsupported { page: self.page.path.clone(), what }
    }

    /// A paragraph that's only a picture is a figure, and the picture links to itself, so one drawn smaller than it is
    /// to fit the page can be opened whole.
    fn paragraph(&mut self, paragraph: &Paragraph) -> Result<String, Error> {
        if let [Node::Image(image)] = paragraph.children.as_slice() {
            let (address, drawn) = self.picture(image)?;
            return Ok(format!("<figure class=\"screen\"><a href=\"{address}\">{drawn}</a></figure>\n"));
        }
        Ok(format!("<p>{}</p>\n", self.all(&paragraph.children)?))
    }

    /// Every heading has what a link to it ends in. All but the title also link to themselves, so a heading's address
    /// can be copied from it, which is why a heading can't hold a link of its own.
    fn heading(&mut self, heading: &Heading) -> Result<String, Error> {
        if holds_a_link(&heading.children) {
            return Err(self.not_drawn(NotDrawn::LinkInHeading));
        }
        let text = self.all(&heading.children)?;
        let slug = escape(self.slugs.next(&plain_all(&heading.children)).as_str());
        let depth = heading.depth;
        if depth == 1 {
            return Ok(format!("<h1 id=\"{slug}\">{text}</h1>\n"));
        }
        Ok(format!("<h{depth} id=\"{slug}\"><a href=\"#{slug}\">{text}</a></h{depth}>\n"))
    }

    fn quote(&mut self, quote: &Blockquote) -> Result<String, Error> {
        let Some((kind, body)) = callout::of(quote) else {
            return Ok(format!("<blockquote>\n{}</blockquote>\n", self.all(&quote.children)?));
        };
        let body = self.all(&body)?;
        let label = kind.label();
        let class = label.to_lowercase();
        Ok(format!("<aside class=\"callout {class}\">\n<p class=\"callout-kind\">{label}</p>\n{body}</aside>\n"))
    }

    /// A list with no blank line between its items, or within one, says each item's paragraph as plain words, as
    /// Markdown has it. Markdown marks a blank line between items on the list, and one within an item on the item.
    fn list(&mut self, list: &List) -> Result<String, Error> {
        let mut items = Vec::new();
        for item in &list.children {
            let Node::ListItem(item) = item else { return Err(self.not_drawn(NotDrawn::NotMade)) };
            items.push(item);
        }
        let tight = !list.spread && items.iter().all(|item| !item.spread);
        let mut drawn = String::new();
        for item in items {
            drawn.push_str(&self.item(item, tight)?);
        }
        Ok(match (list.ordered, list.start) {
            (false, _) => format!("<ul>\n{drawn}</ul>\n"),
            (true, Some(start)) if start != 1 => format!("<ol start=\"{start}\">\n{drawn}</ol>\n"),
            (true, _) => format!("<ol>\n{drawn}</ol>\n"),
        })
    }

    fn item(&mut self, item: &ListItem, tight: bool) -> Result<String, Error> {
        if item.checked.is_some() {
            return Err(self.not_drawn(NotDrawn::Task));
        }
        let mut parts = String::new();
        for part in &item.children {
            if let (Node::Paragraph(paragraph), true) = (part, tight) {
                parts.push_str(&self.all(&paragraph.children)?);
            } else {
                parts.push_str(&self.node(part)?);
            }
        }
        Ok(format!("<li>{parts}</li>\n"))
    }

    fn link(&mut self, link: &Link) -> Result<String, Error> {
        let text = self.all(&link.children)?;
        let href = match target(self.source, &self.page.path, &link.url)? {
            Target::Elsewhere(url) => url,
            Target::Page { page, fragment } => self.page_href(&page, fragment.as_deref())?,
            Target::File { file, fragment: None } => file.on_github(),
            Target::File { file, fragment: Some(fragment) } => format!("{}#{fragment}", file.on_github()),
        };
        Ok(format!("<a class=\"link\" href=\"{}\"{}>{text}</a>", escape(&href), title(link.title.as_deref())))
    }

    /// The address of another page, or of a heading in one, once both are known to be there.
    fn page_href(&self, to: &PagePath, fragment: Option<&str>) -> Result<String, Error> {
        let from = &self.page.path;
        let Some(page) = self.pages.get(to) else {
            return Err(Error::Missing { page: from.clone(), target: to.to_string() });
        };
        let Some(fragment) = fragment else { return Ok(from.href(to)) };
        if !page.has_heading(fragment) {
            return Err(Error::NoHeading { page: from.clone(), target: to.clone(), fragment: fragment.to_string() });
        }
        Ok(if to == from { format!("#{fragment}") } else { format!("{}#{fragment}", from.href(to)) })
    }

    /// Where a picture is published, as this page links to it, and the picture drawn.
    fn picture(&mut self, image: &Image) -> Result<(String, String), Error> {
        let Target::File { file, fragment: None } = target(self.source, &self.page.path, &image.url)? else {
            return Err(self.not_drawn(NotDrawn::PictureElsewhere));
        };
        let shown = self.pictures.show(self.source, &self.page.path, &file)?;
        let address = escape(&format!("{}{}", self.page.path.up(), shown.address));
        let drawn = format!(
            "<img src=\"{address}\" width=\"{}\" height=\"{}\" alt=\"{}\"{} loading=\"lazy\" />",
            shown.width_points,
            shown.height_points,
            escape(&image.alt),
            title(image.title.as_deref())
        );
        Ok((address, drawn))
    }

    /// A table's first row is its headings. It's in a box that scrolls sideways, so a wide one doesn't widen a phone's
    /// page.
    fn table(&mut self, table: &Table) -> Result<String, Error> {
        let mut rows = Vec::new();
        for row in &table.children {
            let Node::TableRow(row) = row else { return Err(self.not_drawn(NotDrawn::NotMade)) };
            let cell = if rows.is_empty() { "th" } else { "td" };
            let mut cells = Vec::new();
            for (column, content) in row.children.iter().enumerate() {
                let Node::TableCell(content) = content else { return Err(self.not_drawn(NotDrawn::NotMade)) };
                let align = match table.align.get(column) {
                    Some(AlignKind::Right) => " style=\"text-align: right\"",
                    Some(AlignKind::Center) => " style=\"text-align: center\"",
                    Some(AlignKind::Left | AlignKind::None) | None => "",
                };
                cells.push(format!("<{cell}{align}>{}</{cell}>", self.all(&content.children)?));
            }
            rows.push(format!("<tr>{}</tr>\n", cells.concat()));
        }
        let Some((head, body)) = rows.split_first() else { return Err(self.not_drawn(NotDrawn::NotMade)) };
        let body = body.concat();
        Ok(format!("<div class=\"table\"><table>\n<thead>\n{head}</thead>\n<tbody>\n{body}</tbody>\n</table></div>\n"))
    }
}

/// What a link or a picture says when it's pointed at, as an attribute, if it was given something to say.
fn title(title: Option<&str>) -> String {
    title.map(|title| format!(" title=\"{}\"", escape(title))).unwrap_or_default()
}

fn holds_a_link(nodes: &[Node]) -> bool {
    nodes.iter().any(|node| matches!(node, Node::Link(_)) || node.children().is_some_and(|inside| holds_a_link(inside)))
}

fn code_block(code: &Code) -> String {
    let language = code.lang.as_deref().map(|lang| format!(" class=\"language-{}\"", escape(lang))).unwrap_or_default();
    format!("<pre><code{language}>{}</code></pre>\n", escape(&code.value))
}

/// `text` as HTML says it, between tags or in an attribute's quotes.
pub(crate) fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            other => escaped.push(other),
        }
    }
    escaped
}
