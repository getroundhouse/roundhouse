use anyhow::{Context as _, Result, anyhow};
use markdown::mdast::{self, Image, ListItem, Node};
use markdown::{Options, ParseOptions};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// How the spec marks what the owner hasn't confirmed, as the HTML of a rule or a caption has it. The mark can end
/// there, or go on to say which part of a rule isn't confirmed, as "*Unconfirmed:* the copper." does.
pub(crate) const UNCONFIRMED: &str = "<em>Unconfirmed";

/// How the spec marks what's decided and not built yet, as the HTML of a rule has it.
pub(crate) const NOT_BUILT: &str = "<em>Not built";

#[derive(Serialize)]
pub(crate) struct Index {
    pub(crate) areas: Vec<Area>,
}

#[derive(Serialize)]
pub(crate) struct Area {
    /// Its file in `docs/spec/`, such as `chats.md`.
    pub(crate) file: String,
    pub(crate) title: String,
    /// What it covers, as the spec's README says.
    pub(crate) covers: String,
    pub(crate) sections: Vec<Section>,
}

#[derive(Serialize)]
pub(crate) struct Section {
    /// `None` for what comes before the area's first heading.
    pub(crate) title: Option<String>,
    /// How deep its heading is, from 2, for one under the area's title, and 2 for what comes before the first.
    pub(crate) depth: u8,
    /// What it says, in order.
    pub(crate) blocks: Vec<Block>,
}

/// A rule, or anything else a section says, such as a paragraph or a table, with the screens that show it. Screens
/// shown on their own, with their own captions, are a block with no words.
#[derive(Serialize)]
pub(crate) struct Block {
    /// The words in bold it starts with, as HTML, which say a rule in short.
    pub(crate) lead: Option<String>,
    /// All its words as HTML, without its screens and their captions.
    pub(crate) html: String,
    /// How many times its words are marked *Unconfirmed*.
    pub(crate) unconfirmed: usize,
    pub(crate) not_built: bool,
    pub(crate) screens: Vec<Screen>,
    /// Its cells, if it's a table.
    pub(crate) table: Option<Table>,
}

/// A table, each cell as HTML, without a paragraph around it.
#[derive(Serialize)]
pub(crate) struct Table {
    pub(crate) header: Vec<String>,
    pub(crate) rows: Vec<Vec<String>>,
}

#[derive(Serialize)]
pub(crate) struct Screen {
    pub(crate) name: ScreenName,
    /// Where the image is from `docs/spec/`, as the spec gives it.
    pub(crate) image: String,
    pub(crate) alt: String,
    /// How big the image is, if it's a PNG, which every screen is.
    pub(crate) size: Option<Size>,
    /// The caption as HTML.
    pub(crate) caption: Option<String>,
    pub(crate) confirmation: Confirmation,
    /// Where it draws each name it shows, if it's one of the gallery's sheets of the foundations, from what
    /// the gallery keeps beside its image, and otherwise empty.
    #[serde(skip_serializing_if = "Places::is_empty")]
    pub(crate) places: Places,
}

/// Where a sheet draws each name it shows: by the type that gives them, such as `look::Space`, as the
/// spec's table of them names it, then by each one's name, such as `Medium`.
pub(crate) type Places = BTreeMap<String, BTreeMap<String, Place>>;

/// A box in an image's pixels: its left and top edges, and how wide and tall it is.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Place {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

/// An image's size in pixels.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Size {
    pub(crate) width: u32,
    pub(crate) height: u32,
}

/// The name of a screen's image without its extension, which is the gallery's name for the screen.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(transparent)]
pub(crate) struct ScreenName(pub(super) String);

impl ScreenName {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Confirmation {
    Confirmed,
    Unconfirmed,
    /// Nothing says whether the owner has confirmed it.
    NoCaption,
}

impl Area {
    pub(crate) fn screens(&self) -> impl Iterator<Item = &Screen> {
        self.sections.iter().flat_map(|section| &section.blocks).flat_map(|block| &block.screens)
    }
}

/// Every area the spec's README lists, in its order, from the spec in `spec`.
pub(crate) fn read(spec: &Path) -> Result<Index> {
    let read = |file: &str| {
        let path = spec.join(file);
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))
    };
    let readme = read("README.md")?;
    let areas = listed(&readme).into_iter().map(|(file, covers)| area(&file, &covers, &read(&file)?));
    let mut areas = areas.collect::<Result<Vec<_>>>()?;
    let blocks = areas.iter_mut().flat_map(|area| area.sections.iter_mut().flat_map(|section| &mut section.blocks));
    for screen in blocks.flat_map(|block| &mut block.screens) {
        let image = spec.join(&screen.image);
        screen.size = png_size(&image);
        screen.places = places(&image.with_extension("json"))?;
    }
    Ok(Index { areas })
}

/// What the gallery keeps at `path`, beside a sheet's image, of where it draws each name, or nothing if it keeps
/// nothing there, as for any screen but a sheet.
fn places(path: &Path) -> Result<Places> {
    let read = || format!("reading {}", path.display());
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).with_context(read),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Places::new()),
        Err(error) => Err(error).with_context(read),
    }
}

/// The size a PNG's header gives, if `path` is one.
fn png_size(path: &Path) -> Option<Size> {
    let mut header = [0; 24];
    std::io::Read::read_exact(&mut std::fs::File::open(path).ok()?, &mut header).ok()?;
    let png = header.starts_with(b"\x89PNG\r\n\x1a\n") && &header[12..16] == b"IHDR";
    let number = |at: usize| header[at..at + 4].try_into().ok().map(u32::from_be_bytes);
    png.then(|| Some(Size { width: number(16)?, height: number(20)? })).flatten()
}

/// The files the README's table lists, each with what it covers, from rows such as `| `chats.md` | Chats |`.
pub(super) fn listed(readme: &str) -> Vec<(String, String)> {
    let row = |line: &str| {
        let (file, covers) = line.strip_prefix("| `")?.split_once("` | ")?;
        let covers = covers.strip_suffix(" |")?;
        let markdown = Path::new(file).extension().is_some_and(|extension| extension == "md");
        markdown.then(|| (file.to_string(), covers.to_string()))
    };
    readme.lines().filter_map(row).collect()
}

/// One area, from the text of its file.
pub(crate) fn area(file: &str, covers: &str, text: &str) -> Result<Area> {
    // The parser panics on some texts, as the comment on it in the workspace's manifest says, so a file that makes it
    // is refused as one it can't read.
    let tree = std::panic::catch_unwind(|| markdown::to_mdast(text, &ParseOptions::gfm()))
        .map_err(|_| anyhow!("{file}: the parser panicked"))?
        .map_err(|message| anyhow!("{file}: {message}"))?;
    let nodes = tree.children().map_or(&[][..], Vec::as_slice);
    let reader = Reader { file, text };
    let mut title = None;
    let mut sections = vec![Section { title: None, depth: 2, blocks: Vec::new() }];
    for (ix, node) in nodes.iter().enumerate() {
        let blocks = match node {
            Node::Heading(heading) if heading.depth == 1 => {
                title = Some(node.to_string());
                continue;
            }
            Node::Heading(heading) => {
                sections.push(Section { title: Some(node.to_string()), depth: heading.depth, blocks: Vec::new() });
                continue;
            }
            _ => &mut sections.last_mut().context("there's always a section")?.blocks,
        };
        match node {
            // Each rule on its own. A numbered list is steps in order, which read together.
            Node::List(list) if !list.ordered => {
                for item in &list.children {
                    let Node::ListItem(item) = item else { continue };
                    blocks.push(reader.item(item)?);
                }
            }
            _ => reader.top(nodes, ix, blocks)?,
        }
    }
    let title = title.with_context(|| format!("{file} has no title"))?;
    Ok(Area {
        file: file.to_string(),
        title,
        covers: covers.to_string(),
        // A heading is kept with nothing under it but deeper headings, so theirs are read under it.
        sections: sections
            .into_iter()
            .filter(|section| section.title.is_some() || !section.blocks.is_empty())
            .collect(),
    })
}

/// Reads blocks from an area's file.
struct Reader<'a> {
    pub(super) file: &'a str,
    pub(super) text: &'a str,
}

impl Reader<'_> {
    /// The block at `ix` in the area, if it says anything, added to `blocks`. A run of images is one block, as it
    /// has one caption.
    pub(super) fn top(&self, nodes: &[Node], ix: usize, blocks: &mut Vec<Block>) -> Result<()> {
        let node = &nodes[ix];
        let in_a_run = ix > 0 && images(&nodes[ix - 1]).is_some();
        if let Some(images) = images(node) {
            let caption = self.caption(nodes, ix)?;
            let screens = images.into_iter().map(|image| screen(image, caption.clone()));
            match blocks.last_mut() {
                Some(run) if in_a_run => run.screens.extend(screens),
                _ => blocks.push(Block { screens: screens.collect(), ..self.block(None, "")? }),
            }
        } else if !captions_the_next(nodes, ix) {
            blocks.push(self.block(Some(node), self.source(node)?)?);
        }
        Ok(())
    }

    /// A rule in a list, with the screens under it.
    pub(super) fn item(&self, item: &ListItem) -> Result<Block> {
        let children = &item.children;
        let mut words = Vec::new();
        let mut screens = Vec::new();
        for (ix, child) in children.iter().enumerate() {
            if let Some(images) = images(child) {
                let caption = self.caption(children, ix)?;
                screens.extend(images.into_iter().map(|image| screen(image, caption.clone())));
            } else if ix == 0 || !captions_the_next(children, ix) {
                // The rule's own words, even when they caption the screens right under them.
                words.push(self.unindented(child)?);
            }
        }
        Ok(Block { screens, ..self.block(children.first(), &words.join("\n\n"))? })
    }

    /// A block of `markdown`, which starts as `first` does.
    pub(super) fn block(&self, first: Option<&Node>, markdown: &str) -> Result<Block> {
        let html = if markdown.is_empty() { String::new() } else { self.html(markdown)? };
        let lead = match first {
            Some(Node::Paragraph(paragraph)) => match paragraph.children.first() {
                Some(strong @ Node::Strong(_)) => Some(self.words(self.source(strong)?)?),
                _ => None,
            },
            _ => None,
        };
        let table = match first {
            Some(Node::Table(table)) => Some(self.table(table)?),
            _ => None,
        };
        Ok(Block {
            lead,
            unconfirmed: html.matches(UNCONFIRMED).count(),
            not_built: html.contains(NOT_BUILT),
            html,
            screens: Vec::new(),
            table,
        })
    }

    /// A table's cells, its heading's first.
    fn table(&self, table: &mdast::Table) -> Result<Table> {
        let mut rows = table.children.iter().map(|row| {
            let cells = row.children().map_or(&[][..], Vec::as_slice);
            cells.iter().map(|cell| self.cell(cell)).collect::<Result<Vec<_>>>()
        });
        let header = rows.next().transpose()?.unwrap_or_default();
        Ok(Table { header, rows: rows.collect::<Result<_>>()? })
    }

    /// A cell's words as HTML, from the first to the last, without the spaces and the bars around them.
    fn cell(&self, cell: &Node) -> Result<String> {
        let words = cell.children().map_or(&[][..], Vec::as_slice);
        let (Some(first), Some(last)) = (words.first(), words.last()) else { return Ok(String::new()) };
        let at = |node: &Node| node.position().context("the parser gives every node a position").cloned();
        let source = self.text.get(at(first)?.start.offset..at(last)?.end.offset).context("a cell is in its file")?;
        self.words(source)
    }

    /// Markdown that's words alone, such as a cell's or a rule's first words, as HTML, without the paragraph the
    /// renderer puts them in, since they go among other words.
    fn words(&self, markdown: &str) -> Result<String> {
        let html = self.html(markdown)?;
        Ok(html.trim_end().trim_start_matches("<p>").trim_end_matches("</p>").to_string())
    }

    /// The caption, as HTML, of the images at `ix`: the paragraph right above them, or above the run of images
    /// they're in, if it starts in bold.
    pub(super) fn caption(&self, nodes: &[Node], ix: usize) -> Result<Option<String>> {
        let above = nodes[..ix].iter().rev().find(|node| images(node).is_none());
        above.filter(|above| starts_in_bold(above)).map(|above| self.html(self.source(above)?)).transpose()
    }

    /// The node's own text in the file.
    pub(super) fn source(&self, node: &Node) -> Result<&str> {
        let position = node.position().context("the parser gives every block a position")?;
        self.text.get(position.start.offset..position.end.offset).context("a block's position is in its file")
    }

    /// A block in a list item as it would read at the left margin: its lines after the first are indented as far as
    /// the item's words, so that much goes from each.
    pub(super) fn unindented(&self, node: &Node) -> Result<String> {
        let indent = node.position().context("the parser gives every block a position")?.start.column - 1;
        let mut lines = self.source(node)?.lines();
        let first = lines.next().unwrap_or_default();
        let rest = lines.map(|line| {
            let spaces = line.len() - line.trim_start_matches(' ').len();
            &line[spaces.min(indent)..]
        });
        Ok(std::iter::once(first).chain(rest).collect::<Vec<_>>().join("\n"))
    }

    pub(super) fn html(&self, markdown: &str) -> Result<String> {
        // As in `area`, a panic is refused as an error.
        std::panic::catch_unwind(|| markdown::to_html_with_options(markdown, &Options::gfm()))
            .map_err(|_| anyhow!("{}: the parser panicked", self.file))?
            .map_err(|message| anyhow!("{}: {message}", self.file))
    }
}

fn screen(image: &Image, caption: Option<String>) -> Screen {
    let name = Path::new(&image.url).file_stem().map_or_else(|| image.url.clone(), |s| s.to_string_lossy().into());
    let confirmation = match &caption {
        None => Confirmation::NoCaption,
        Some(caption) if caption.contains(UNCONFIRMED) => Confirmation::Unconfirmed,
        Some(_) => Confirmation::Confirmed,
    };
    Screen {
        name: ScreenName(name),
        image: image.url.clone(),
        alt: image.alt.clone(),
        size: None,
        caption,
        confirmation,
        places: Places::new(),
    }
}

/// The images, if the block is a paragraph of nothing else.
fn images(block: &Node) -> Option<Vec<&Image>> {
    let Node::Paragraph(paragraph) = block else { return None };
    let mut images = Vec::new();
    for child in &paragraph.children {
        match child {
            Node::Image(image) => images.push(image),
            Node::Text(text) if text.value.trim().is_empty() => {}
            _ => return None,
        }
    }
    (!images.is_empty()).then_some(images)
}

/// Whether the block at `ix` is the caption of images right below it.
fn captions_the_next(blocks: &[Node], ix: usize) -> bool {
    starts_in_bold(&blocks[ix]) && blocks.get(ix + 1).is_some_and(|next| images(next).is_some())
}

fn starts_in_bold(block: &Node) -> bool {
    matches!(block, Node::Paragraph(paragraph) if matches!(paragraph.children.first(), Some(Node::Strong(_))))
}
