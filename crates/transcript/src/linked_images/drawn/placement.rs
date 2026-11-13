use super::super::LinkedImages;
use crate::images_that_open;
use gpui_kit::component::text::markdown_ast::{Delete, Emphasis, Image, Link, LinkReference, Node, Strong};
use gpui_kit::component::text::{
    InlineElement, InlineRenderContext, MarkdownExtensions, MarkdownNode, MarkdownParseContext, MarkdownPlugin,
};
use gpui_kit::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, RenderImage,
    Styled as _, TestSupportExt as _, Window, div, px,
};
use look::pixels;
use parking_lot::Mutex;
use protocol::event::LinkedImage;
use protocol::model::Seq;
use std::collections::HashSet;
use std::sync::Arc;

/// The names the kit knows each kind of node by.
pub(super) const ALONE: &str = "linked-image-alone";
pub(super) const AMONG_WORDS: &str = "linked-image-among-words";

/// How tall an image among words is, for a line: what the kit draws one at, in its `inline_image_size_for_line`.
const AMONG_WORDS_SHARE: f32 = 0.75;

/// Whether parsing `source` with [`to_read`] can take anything, which it can only if it has an image in Markdown, and
/// each of those starts `![`. One with none is parsed the same with it as without it.
pub(crate) fn may_take(source: &str) -> bool {
    source.contains("![")
}

/// What the kit parses a text with, from byte `from` of it on, for the transcript to draw the images it points to. It's
/// made for each parse, since what reads a paragraph and what reads its images share what's noted of that one. Only
/// the end of the chat parses a text again from near its end, and it has no images to look up, so every text parsed
/// with this is parsed from its start, but a parse of the rest of one is right too, so the parse doesn't depend on
/// who asks for it.
pub(crate) fn to_read(from: usize) -> Arc<MarkdownExtensions> {
    let reading = Arc::new(Reading { from, among_words: Mutex::default() });
    let paragraphs = Arc::clone(&reading);
    let read = MarkdownExtensions::default()
        .block_parser(move |node: &Node, cx: &MarkdownParseContext<'_>| paragraphs.paragraph(node, cx))
        .plugin(ReadsAmongWords(reading));
    Arc::new(read)
}

/// Where the image `node` is starts in its text, if it's one [`to_read`] took, for a test of what a text was parsed as.
#[cfg(test)]
pub(crate) fn taken_at(node: &MarkdownNode) -> Option<usize> {
    node.data::<Pointed>().map(|image| image.at)
}

/// What the kit draws a text read that way with, which names `kept`, in the row of the event at `seq`.
pub(in super::super) fn to_draw(linked: &LinkedImages, seq: Seq, kept: &[LinkedImage]) -> MarkdownExtensions {
    let lookup = Lookup { linked: linked.clone(), seq, kept: kept.into() };
    let alone = lookup.clone();
    MarkdownExtensions::default()
        .block_renderer(ALONE, move |node: &MarkdownNode, _: &mut Window, cx: &mut App| alone.alone(node, cx))
        .plugin(DrawsAmongWords(lookup))
}

/// One parse of a text, from byte `from` of it on, which is where a text that goes on is parsed again from.
struct Reading {
    pub(super) from: usize,
    /// Where each image among a paragraph's words starts in the text, noted as its paragraph is read, for the image to
    /// be taken when it's read. That relies on the kit offering a paragraph before anything in it. If it didn't, none
    /// would be noted, and each would be left to the kit, which opens nothing.
    pub(super) among_words: Mutex<HashSet<usize>>,
}

/// An image the text points to, by the address it gives, and where it starts in the text, which no other image in the
/// text shares, so it names the image's element.
pub(super) struct Pointed {
    pub(super) address: String,
    pub(super) at: usize,
}

impl Pointed {
    pub(super) fn element(&self) -> ElementId {
        ElementId::Name(format!("linked-image/{}", self.at).into())
    }
}

impl Reading {
    /// Takes a paragraph that's only an image, and notes the images among another's words.
    pub(super) fn paragraph(&self, node: &Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let Node::Paragraph(paragraph) = node else { return None };
        match Holds::of(&paragraph.children) {
            Holds::OnlyAnImage(image) => Some(MarkdownNode::new(ALONE, self.pointed(image, cx)?)),
            Holds::Words(images) => {
                self.among_words.lock().extend(images.into_iter().filter_map(|image| self.starts_at(image, cx)));
                None
            }
            Holds::TheKits => None,
        }
    }

    /// Takes an image noted as among its paragraph's words.
    pub(super) fn among_words(&self, node: &Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let Node::Image(image) = node else { return None };
        let pointed = self.pointed(image, cx).filter(|pointed| self.among_words.lock().contains(&pointed.at))?;
        // What the kit draws in its place if it can't set it on the line, and what it's called to assistive apps.
        // The kit takes the image's Markdown for one that says nothing.
        let says = if image.alt.trim().is_empty() { " " } else { image.alt.as_str() };
        Some(MarkdownNode::new(AMONG_WORDS, pointed).text(says.to_string()))
    }

    pub(super) fn pointed(&self, image: &Image, cx: &MarkdownParseContext<'_>) -> Option<Pointed> {
        Some(Pointed { address: image.url.clone(), at: self.starts_at(image, cx)? })
    }

    /// Where `image` starts in the whole text.
    pub(super) fn starts_at(&self, image: &Image, cx: &MarkdownParseContext<'_>) -> Option<usize> {
        Some(self.from + cx.offset() + image.position.as_ref()?.start.offset)
    }
}

/// The kit takes what reads something in a line and what draws it only together, as a plugin, where it takes a
/// block's apart. So an image among words has a plugin to read it and one to draw it, known by the same name, and
/// neither does the other's half.
struct ReadsAmongWords(pub(super) Arc<Reading>);

impl MarkdownPlugin for ReadsAmongWords {
    fn name(&self) -> &str {
        AMONG_WORDS
    }

    fn parse(&self, node: &Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        self.0.among_words(node, cx)
    }
}

struct DrawsAmongWords(pub(super) Lookup);

impl MarkdownPlugin for DrawsAmongWords {
    fn name(&self) -> &str {
        AMONG_WORDS
    }

    fn parse(&self, _: &Node, _: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        None
    }

    fn render_inline(
        &self,
        node: &MarkdownNode,
        context: &InlineRenderContext,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<InlineElement> {
        let image = node.data::<Pointed>()?;
        let height = context.line_height() * AMONG_WORDS_SHARE;
        // Left to the kit, something that isn't its own image sits on the bottom of the text.
        let text = context.text_style().to_run(1);
        let text = window.text_system().shape_line(" ".into(), context.font_size(), &[text], None);
        let baseline = in_the_middle(height, text.ascent, text.descent);
        Some(InlineElement::new(self.0.among_words(image, height, cx)).with_baseline(baseline))
    }
}

/// Where a text's images are looked up, to draw them.
#[derive(Clone)]
struct Lookup {
    pub(super) linked: LinkedImages,
    pub(super) seq: Seq,
    pub(super) kept: Arc<[LinkedImage]>,
}

impl Lookup {
    /// The image that's all its paragraph holds, as wide as it is, up to the column's.
    pub(super) fn alone(&self, node: &MarkdownNode, cx: &mut App) -> AnyElement {
        let drawn = node.data::<Pointed>().and_then(|image| {
            let (ready, covers) = self.ready(image, cx)?;
            Some(images_that_open::whole(div().id(image.element()), ready, covers))
        });
        // Nothing while it loads, or if it can't be shown, as the kit draws one.
        drawn.unwrap_or_else(|| div().into_any_element())
    }

    /// An image among words, `height` tall, or the room the kit leaves one while it loads or if it can't be shown.
    pub(super) fn among_words(&self, image: &Pointed, height: Pixels, cx: &mut App) -> AnyElement {
        let Some((ready, covers)) = self.ready(image, cx) else { return div().size(height).into_any_element() };
        images_that_open::opening(div().id(image.element()), ready.clone(), covers)
            .w(height * (pixels(covers.0) / pixels(covers.1)))
            .h(height)
            .child(images_that_open::picture(ready).size_full())
            .test_support()
            .into_any_element()
    }

    /// The image and how many pixels it covers, once it's here, which starts it loading the first time it's asked
    /// for.
    pub(super) fn ready(&self, image: &Pointed, cx: &mut App) -> Option<(Arc<RenderImage>, (u32, u32))> {
        let ready = self.linked.ready(&image.address, &self.kept, self.seq, cx)?;
        let covers = covers(&ready)?;
        Some((ready, covers))
    }
}

/// What a paragraph holds, for how its images are drawn, as the kit has it: whole, each as wide as it is, when it
/// holds only images, and at the height of the text when it holds words too. The kit looks past bold, italics and
/// struck text for both, so `**![The board](board.png)**` is an image alone.
pub(super) enum Holds<'a> {
    /// One image and nothing else.
    OnlyAnImage(&'a Image),
    /// Words, and these images among them. One in a link isn't among them.
    Words(Vec<&'a Image>),
    /// Anything else, left to the kit to draw as it does: several images and no words, as in one mark or in marks side
    /// by side, which `crate::images_apart` doesn't set apart, any that's a link or refers to its address, and any
    /// beside a tag of HTML, which only the kit can say the words of, and draws whole, or beside `<br>` at the height
    /// of the text.
    TheKits,
}

/// What's in a paragraph, past its marks and its links.
#[derive(Default)]
struct Inside<'a> {
    /// The images that aren't in a link.
    pub(super) images: Vec<&'a Image>,
    pub(super) words: bool,
    /// An image in a link or one that refers to its address, which is the kit's, or a tag of HTML.
    pub(super) the_kits: bool,
}

impl<'a> Holds<'a> {
    /// What a paragraph of `children` holds.
    pub(super) fn of(children: &'a [Node]) -> Self {
        let mut inside = Inside::default();
        inside.read(children, false);
        match inside {
            Inside { images, words: true, .. } => Self::Words(images),
            Inside { images, the_kits: false, .. } if images.len() == 1 => Self::OnlyAnImage(images[0]),
            Inside { .. } => Self::TheKits,
        }
    }
}

impl<'a> Inside<'a> {
    pub(super) fn read(&mut self, children: &'a [Node], in_a_link: bool) {
        for child in children {
            if let Node::Emphasis(Emphasis { children, .. })
            | Node::Strong(Strong { children, .. })
            | Node::Delete(Delete { children, .. }) = child
            {
                self.read(children, in_a_link);
            } else if let Node::Link(Link { children, .. }) | Node::LinkReference(LinkReference { children, .. }) =
                child
            {
                self.read(children, true);
            } else if let (Node::Image(image), false) = (child, in_a_link) {
                self.images.push(image);
            } else if matches!(child, Node::Image(_) | Node::ImageReference(_) | Node::Html(_)) {
                self.the_kits = true;
            } else {
                self.words = true;
            }
        }
    }
}

/// Where the baseline of something `height` tall is, for it to sit in the middle of a line of text with this ascent
/// and descent, which is where the kit's `inline_flow` sets an image: as far above the text's baseline as the middle
/// of the text's own height is. Each platform's text system gives a descent as a size, and the one headless tests
/// draw with gives it below zero, so it's read as a size either way. With a baseline outside it, the kit would draw
/// what the element says in its place.
pub(super) fn in_the_middle(height: Pixels, ascent: Pixels, descent: Pixels) -> Pixels {
    ((height + ascent - descent.abs()) / 2.).clamp(px(0.), height)
}

/// How many pixels it covers, or `None` for one with none, which has no shape to draw it at.
fn covers(image: &RenderImage) -> Option<(u32, u32)> {
    let pixels = image.size(0);
    let side = |pixels: i32| u32::try_from(pixels).ok().filter(|side| *side > 0);
    Some((side(pixels.width.0)?, side(pixels.height.0)?))
}
