//! One of the chat's texts that `crate::prose` draws, from its parsed blocks, with what's selected and what's found
//! painted behind it. Once it's laid out, it tells the selection where its leaves are, and the match being shown where
//! it is.

use std::collections::HashMap;
use std::rc::Rc;

use gpui_kit::base::TextViewDefaults;
use gpui_kit::base::text::editor_host::{BlockNode, FlowPosition, LeafGeometry, RenderContext, collect_leaf_geometry};
use gpui_kit::{
    AnyElement, App, Bounds, CursorStyle, Element, ElementId, GlobalElementId, Hitbox, HitboxBehavior, Hsla,
    InspectorElementId, IntoElement, LayoutId, ParentElement as _, Pixels, Styled as _, Window, div, px, size,
};

use super::parsing::Apart;
use super::{Images, Parsed, Prose, ProseId, TextAt};
use crate::find::Marks;
use crate::selection::Selection;

pub(super) struct ProseElement {
    /// What the text is known by, which its element is too.
    pub(super) known: ProseId,
    pub(super) at: TextAt,
    /// The area that scrolls it, if it's in one.
    pub(super) area: Option<ElementId>,
    /// What it was parsed as, whose blocks it draws, or nothing until it's parsed on another thread.
    pub(super) text: Option<Rc<Parsed>>,
    /// How it's to be parsed on another thread, if it is, which it's drawn as `text` until it's in.
    pub(super) apart: Option<Apart>,
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
        Some(self.known.0.clone())
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
        // In the colour of what it's in, such as a thought's muted one, as the kit's text view is, since the kit's
        // defaults have it follow that. The kit draws its other colours otherwise on a surface whose text is far
        // lighter or darker than its own, which none of the transcript's is, so only the colour is followed.
        let color = window.text_style().color;
        let follows = cx.try_global::<TextViewDefaults>().is_some_and(TextViewDefaults::inherit_text_color);
        if follows && color != context.style().foreground() {
            let style = context.style().clone().with_foreground(color);
            context = context.with_style(style);
        }
        if let Some(parsed) = &self.text {
            for (identifier, link) in &parsed.links {
                context = context.with_link_reference(identifier.clone(), link.clone());
            }
        }
        let style = context.style();
        // As the kit's text view lays its blocks out, in its text's colour.
        let mut element = match &self.text {
            Some(parsed) => div()
                .w_full()
                .text_color(style.foreground())
                .child(div().children(blocks(&parsed.blocks, &context, window, cx)))
                .into_any_element(),
            None => div().into_any_element(),
        };
        if let Some(apart) = self.apart.take() {
            self.prose.parse_apart(self.known.clone(), apart, cx);
        }
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
        let Some(parsed) = &self.text else {
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
        // What shows of it, which an area that scrolls it, or the list, can cut short.
        let shows = bounds.intersect(&window.content_mask().bounds);
        self.selection.drawn(self.known.clone(), self.at, shows, self.area.clone(), leaves);
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
        reveal.drawn_at(Bounds::new(at, size(px(1.), geometry.line_height())), self.area.clone());
    }
}

/// `blocks`, each laid out as the kit's text view lays it out among the others. Each is known by its place among them,
/// since where the kit's parser says a block is in a text that went on counts from where it was parsed again from, and
/// the kit would know blocks by that.
fn blocks(blocks: &[BlockNode], context: &RenderContext, window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let all: Vec<&BlockNode> = blocks.iter().collect();
    let last = all.len().saturating_sub(1);
    (blocks.iter().enumerate())
        .map(|(ix, block)| {
            block.render_with_id(("block", ix), FlowPosition::at(&all, ix, ix == last), context, window, cx)
        })
        .collect()
}
