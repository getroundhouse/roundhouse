//! One of the agent's texts, drawn from its parsed blocks, with what's selected and what's found painted behind it.
//! Once it's laid out, it tells the selection where its leaves are, and the match being shown where it is.

use std::collections::HashMap;
use std::rc::Rc;

use gpui_kit::base::text::editor_host::{BlockNode, FlowPosition, LeafGeometry, RenderContext, collect_leaf_geometry};
use gpui_kit::{
    AnyElement, App, Bounds, CursorStyle, Element, ElementId, GlobalElementId, Hitbox, HitboxBehavior, Hsla,
    ImageSource, InspectorElementId, IntoElement, LayoutId, ParentElement as _, Pixels, SharedUri, Styled as _, Window,
    div, px, size,
};

use super::{Parsed, Prose, TextAt};
use crate::find::Marks;
use crate::selection::Selection;

pub(super) type Images = Box<dyn Fn(&SharedUri) -> ImageSource + Send + Sync>;

/// What a text draws: its blocks, or until it's parsed on another thread, `meanwhile`, or nothing if that's `None`.
pub(super) enum Drawing {
    Parsed(Rc<Parsed>),
    Unparsed { source: String, meanwhile: Option<AnyElement> },
}

pub(super) struct ProseElement {
    pub(super) id: ElementId,
    pub(super) at: TextAt,
    pub(super) text: Drawing,
    /// What's found in each leaf, by key.
    pub(super) marks: Vec<Marks>,
    pub(super) images: Option<Images>,
    pub(super) prose: Prose,
    pub(super) selection: Selection,
}

impl IntoElement for ProseElement {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for ProseElement {
    /// The blocks, laid out, and the colour a selection is drawn in.
    type RequestLayoutState = (AnyElement, Hsla);
    /// Where the pointer is over it, which shows it's text to select.
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut context = RenderContext::from_app(cx).on_link_click(self.selection.on_link());
        if let Some(images) = self.images.take() {
            context = context.with_image_source(images);
        }
        let style = context.style();
        let mut element = match &mut self.text {
            // As the kit's text view lays its blocks out, in its text's colour.
            Drawing::Parsed(parsed) => div()
                .w_full()
                .text_color(style.foreground())
                .child(div().children(blocks(&parsed.blocks, &context, window, cx)))
                .into_any_element(),
            Drawing::Unparsed { source, meanwhile } => {
                self.prose.parse_apart(self.at, source, cx);
                meanwhile.take().unwrap_or_else(|| div().into_any_element())
            }
        };
        (element.request_layout(window, cx), (element, style.selection()))
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (element, selection_color): &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Hitbox {
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let Drawing::Parsed(parsed) = &self.text else {
            element.prepaint(window, cx);
            return hitbox;
        };
        // What's found first, so what's selected is drawn over it.
        let mut behind: HashMap<u64, Vec<(std::ops::Range<usize>, Hsla)>> = HashMap::new();
        for (leaf, marks) in (0u64..).zip(&self.marks).filter(|(_, marks)| !marks.is_empty()) {
            behind.entry(leaf).or_default().extend(marks.backgrounds());
        }
        for (leaf, range) in self.selection.selected_in(self.at, &parsed.leaves) {
            behind.entry(leaf).or_default().push((range, *selection_color));
        }
        let (_, leaves) = collect_leaf_geometry(behind, || element.prepaint(window, cx));
        self.reveal(&leaves);
        self.selection.drawn(self.at, bounds, leaves);
        hitbox
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        (element, _): &mut Self::RequestLayoutState,
        hitbox: &mut Hitbox,
        window: &mut Window,
        cx: &mut App,
    ) {
        // Before the text's own, so a link's pointer wins.
        window.set_cursor_style(CursorStyle::IBeam, hitbox);
        element.paint(window, cx);
    }
}

impl ProseElement {
    /// Says where the match being shown was drawn, if it's here.
    fn reveal(&self, leaves: &HashMap<u64, LeafGeometry>) {
        let here = (0u64..).zip(&self.marks).find_map(|(leaf, marks)| Some((leaf, marks.here()?, marks.reveal()?)));
        let Some((leaf, range, reveal)) = here else { return };
        let Some(geometry) = leaves.get(&leaf) else { return };
        let at = geometry.position_for_index(range.start);
        reveal.drawn_at(Bounds::new(at, size(px(1.), geometry.line_height())), None);
    }
}

/// `blocks`, each laid out as the kit's text view lays it out among the others.
fn blocks(blocks: &[BlockNode], context: &RenderContext, window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let all: Vec<&BlockNode> = blocks.iter().collect();
    let last = all.len().saturating_sub(1);
    (blocks.iter().enumerate())
        .map(|(ix, block)| block.render(ix, FlowPosition::at(&all, ix, ix == last), context, window, cx))
        .collect()
}
