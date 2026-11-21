use std::rc::Rc;
use std::time::Instant;

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, ElementId, FocusHandle, Focusable, IntoElement, ListAlignment,
    ListOffset, ListState, ParentElement as _, Pixels, Render, SharedString, Styled as _, Task, Window, div, list, px,
};

use super::changes::{Change, changed};
use crate::rows::in_column;
use crate::selection::{Scrolled, Scroller, Selection};
use crate::texts::{Of, TextAt, Texts, TopBlock, Written, top_blocks};

/// How far past the window the list draws blocks, so one scrolled into sight is drawn already.
const OVERDRAW: f32 = 400.;

/// A Markdown text, scrolled in the chat's column, with what's selected of it, which a copy takes. It's cut into the
/// blocks at its top, and only those in sight are drawn, each as a text of its own, since a document drawn whole costs
/// every frame what all its blocks do. Drawn whole, a 14 KB README took about 21 ms a frame and a 228 KB one 110 ms.
/// Drawn a block at a time, the 228 KB one took about 1.3 ms.
pub struct Document {
    texts: Texts,
    selection: Selection,
    focus: FocusHandle,
    pub(super) list: ListState,
    shown: Option<Shown>,
    /// Cutting the text it's to show next into its blocks, on another thread.
    cutting: Option<Task<()>>,
}

/// The text a document shows, by its blocks, each with how it changed if it shows a change, and what it's known by,
/// such as a file's path.
struct Shown {
    name: SharedString,
    blocks: Rc<[(TopBlock, Change)]>,
}

impl Focusable for Document {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Document {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        let (texts, selection) = texts(&focus, cx);
        let list = ListState::new(0, ListAlignment::Top, px(OVERDRAW));
        Self { texts, selection, focus, list, shown: None, cutting: None }
    }

    /// Shows `text`, known as `name`, such as the path of the file it's read from, once it's cut into its blocks. Another
    /// name starts it at its top, and the same one, read again, keeps where it's scrolled to. What was selected goes,
    /// since the blocks it was in may not be there now.
    pub fn show(&mut self, name: SharedString, text: SharedString, cx: &mut Context<Self>) {
        self.cut(name, cx, move || top_blocks(&text).into_iter().map(|block| (block, Change::Kept)).collect());
    }

    /// Shows what changed from `was` to `is`, two versions of the text known as `name`, as [`show`](Self::show) shows
    /// one: the blocks both have, and each that only one has, where it has it, with what was removed struck through
    /// on red, and what was added on green. A block that changed shows as it was, then as it is.
    pub fn show_changes(&mut self, name: SharedString, was: SharedString, is: SharedString, cx: &mut Context<Self>) {
        self.cut(name, cx, move || changed(&was, &is));
    }

    /// Shows the blocks `cut` makes on another thread, known as `name`, once it's made them.
    fn cut(
        &mut self,
        name: SharedString,
        cx: &mut Context<Self>,
        cut: impl FnOnce() -> Vec<(TopBlock, Change)> + Send + 'static,
    ) {
        // Another's blocks aren't shown under its name while it's cut.
        if self.shown.as_ref().is_some_and(|shown| shown.name != name) {
            self.shown = None;
            self.list.reset(0);
            cx.notify();
        }
        let cutting = cx.background_spawn(async move { cut() });
        self.cutting = Some(cx.spawn(async move |this, cx| {
            let blocks: Rc<[(TopBlock, Change)]> = cutting.await.into();
            let _ = this.update(cx, |this, cx| this.shown(name, blocks, cx));
        }));
    }

    fn shown(&mut self, name: SharedString, blocks: Rc<[(TopBlock, Change)]>, cx: &mut Context<Self>) {
        let (texts, selection) = texts(&self.focus, cx);
        for (ix, (block, _)) in blocks.iter().enumerate() {
            texts.read_ahead(element(&name, ix), at(ix), block.leaves.clone());
        }
        (self.texts, self.selection) = (texts, selection);
        // The list's splice moves where it's scrolled to the top of what it replaces, so it's put back.
        let was = self.shown.as_ref().is_some_and(|shown| shown.name == name).then(|| self.list.logical_scroll_top());
        self.list.reset(blocks.len());
        if let Some(top) = was {
            self.list.scroll_to(top);
        }
        self.shown = Some(Shown { name, blocks });
        cx.notify();
    }

    /// What a copy of all of it has, as it's drawn.
    #[cfg(feature = "testing")]
    pub fn copied_whole(&self) -> String {
        self.selection.all_as_copied()
    }

    /// The `ix`th block, in the chat's column, marked with how it changed, with room above the first and below the
    /// last.
    fn block(&self, ix: usize, cx: &App) -> AnyElement {
        let Some(shown) = &self.shown else { return div().into_any_element() };
        let Some((block, change)) = shown.blocks.get(ix) else { return div().into_any_element() };
        // One marked has no gap above or below it, which would be tinted, and its marks keep it apart.
        let flow =
            if *change == Change::Kept { block.flow } else { block.flow.map(|flow| flow.first(true).last(true)) };
        let written =
            Written::unfound(at(ix), element(&shown.name, ix), &block.source).block_of_a_text(flow, block.shown);
        let text = self.texts.drawing(Of::Document(ix), || self.texts.draw(&self.selection, &written, None));
        let kept_next = shown.blocks.get(ix + 1).is_some_and(|(_, next)| *next == Change::Kept);
        let column = in_column(marked(text, *change, kept_next, cx));
        let column = if ix == 0 { column.pt(look::Space::XLarge) } else { column };
        let last = ix + 1 == shown.blocks.len();
        if last { column.pb(look::Space::XLarge) } else { column }.into_any_element()
    }
}

impl Render for Document {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let blocks = list(self.list.clone(), cx.processor(|this, ix, _, cx| this.block(ix, cx))).size_full();
        let selecting = Scrolled::new("document-frames", Blocks(self.list.clone()), &self.selection, blocks);
        self.selection.with_keys(div().size_full().child(selecting))
    }
}

/// The list of a document's blocks, as what scrolls its texts, each of which says it's in its block.
struct Blocks(ListState);

impl Scroller for Blocks {
    fn scroll_down(&self, by: Pixels) {
        self.0.scroll_by(by);
    }

    fn scrolled(&self) -> Pixels {
        -self.0.scroll_px_offset_for_scrollbar().y
    }

    fn to_start(&self) {
        self.0.scroll_to(ListOffset::default());
    }

    fn to_end(&self, _: Instant) {
        self.0.scroll_to_end();
    }

    fn show(&self, of: Of) -> bool {
        let Of::Document(block) = of else { return false };
        let out_of_view = |out: Option<bool>| out != Some(false);
        let shows =
            !out_of_view(self.0.item_is_above_viewport(block)) && !out_of_view(self.0.item_is_below_viewport(block));
        if !shows {
            self.0.scroll_to_reveal_item(block);
        }
        !shows
    }
}

/// A block, marked with how it changed: one removed struck through on red, and one added on green, each with a bar
/// down its left edge, and a paragraph's gap below if `kept_next`, a block that didn't change, is after it.
fn marked(text: AnyElement, change: Change, kept_next: bool, cx: &App) -> AnyElement {
    let (colors, struck) = match change {
        Change::Kept => return text,
        Change::Removed => (look::colors(cx), true),
        Change::Added => (look::colors(cx), false),
    };
    let tint = if struck { colors.removed } else { colors.added };
    let block = div()
        .w_full()
        .my(px(2.))
        .pl(look::Space::Medium)
        .pr(look::Space::Small)
        .py(look::Space::XSmall)
        .border_l(look::QUOTE_RULE)
        .border_color(tint)
        .bg(tint.opacity(0.12))
        .rounded_r(px(4.))
        .when(kept_next, |block| block.mb(look::Space::Medium));
    let block = if struck { block.text_color(colors.text_muted).line_through() } else { block };
    block.child(text).into_any_element()
}

/// Texts of a document's own, and their selection, which takes the keys while `focus` has them.
fn texts(focus: &FocusHandle, cx: &mut Context<Document>) -> (Texts, Selection) {
    let this = cx.weak_entity();
    // A long block is parsed on another thread, and drawn once it's in, at its own height.
    let texts = Texts::new(move |of, cx| {
        let _ = this.update(cx, |document, cx| {
            if let Of::Document(ix) = of {
                // Measured again, keeping where in it the list is scrolled to, as a splice wouldn't.
                document.list.remeasure_items(ix..ix + 1);
            }
            cx.notify();
        });
    });
    (texts.clone(), Selection::new(texts, focus.clone()))
}

/// Where the `ix`th block is, among a document's.
fn at(ix: usize) -> TextAt {
    TextAt::first_of(Of::Document(ix))
}

/// What the `ix`th block of the document known as `name` is known by.
fn element(name: &SharedString, ix: usize) -> ElementId {
    ElementId::NamedInteger(name.clone(), u64::try_from(ix).unwrap_or(u64::MAX))
}
