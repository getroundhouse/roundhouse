//! One changed file's diff, on the kit's `Diff`, as the workspace's file tab shows it, with the comments on it and the
//! one being written. Which file is the pane's to say, since the file tab is the pane's, and the files and comments
//! are the workspace's [`Review`]'s.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::diff::{
    Diff, DiffAnnotation, DiffFile, DiffLinePosition, DiffMode, DiffSide as KitSide, DiffState,
};
use gpui_kit::{
    AnyElement, App, AppContext, Context, ElementId, Entity, EventEmitter, FocusHandle, Focusable, Hsla,
    InteractiveElement as _, IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Window, div,
};
use protocol::model::DiffSide;

use crate::review::{Review, ReviewEvent};
use annotations::{comment_card, draft_card};
pub use draft::Draft;

mod annotations;
mod draft;

#[derive(Clone, Debug)]
pub enum FileDiffEvent {
    /// Put this in the chat's composer.
    AddToChat(String),
}

fn kit_side(side: DiffSide) -> KitSide {
    match side {
        DiffSide::Old => KitSide::Original,
        DiffSide::New | DiffSide::Unknown => KitSide::Modified,
    }
}

fn our_side(side: KitSide) -> DiffSide {
    match side {
        KitSide::Original => DiffSide::Old,
        KitSide::Modified => DiffSide::New,
    }
}

pub struct FileDiff {
    review: Entity<Review>,
    /// The file the file tab shows, while it's open.
    pub path: Option<SharedString>,
    /// The kit's view of it, once there's been one.
    pub state: Option<Entity<DiffState>>,
    /// The comment being written. It's kept while another file shows, and shows again with its own.
    pub draft: Option<Draft>,
    /// Where keys go while the file tab shows, when no comment is being written, so the window's keys reach the app.
    focus: FocusHandle,
    _subscriptions: [Subscription; 2],
}

impl EventEmitter<FileDiffEvent> for FileDiff {}

impl Focusable for FileDiff {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl FileDiff {
    pub fn new(review: Entity<Review>, cx: &mut Context<Self>) -> Self {
        // Comments show as they come and go, and the file again once the diff has loaded again.
        let observing = cx.observe(&review, |_, _, cx| cx.notify());
        let loading = cx.subscribe(&review, |t, _, event: &ReviewEvent, cx| {
            if matches!(event, ReviewEvent::Loaded) {
                t.show_file(cx);
            }
        });
        let focus = cx.focus_handle();
        Self { review, path: None, state: None, draft: None, focus, _subscriptions: [observing, loading] }
    }

    /// Shows a file, in place of the one it showed, or none, as the file tab says.
    pub fn show(&mut self, path: Option<SharedString>, cx: &mut Context<Self>) {
        if self.path != path {
            self.path = path;
            self.show_file(cx);
        }
        cx.notify();
    }

    /// The file it shows, as the diff last loaded has it. `None` once it no longer changes anything.
    pub fn file<'a>(&self, cx: &'a App) -> Option<&'a DiffFile> {
        let path = self.path.as_ref()?;
        self.review.read(cx).file(path)
    }

    /// Gives the kit the file's diff as it is now.
    fn show_file(&mut self, cx: &mut Context<Self>) {
        let Some(file) = self.file(cx).cloned() else { return };
        if let Some(state) = &self.state {
            state.update(cx, |s, cx| s.set_files([file], cx));
            // New files clear the selection, which is what marks the line being commented on.
            self.select_draft_line(cx);
        } else {
            let state = cx.new(|cx| {
                DiffState::new([file], cx).with_mode(DiffMode::Unified).with_syntax_highlighter(look::highlight)
            });
            self.state = Some(state);
        }
    }

    /// The comments on the file, and the one being written if it's on this file. One being edited shows only as
    /// it's being written.
    fn annotations(&self, cx: &App) -> Vec<DiffAnnotation> {
        let showing = |path: &str| self.path.as_ref().is_some_and(|open| open.as_ref() == path);
        let editing = self.draft.as_ref().and_then(|draft| draft.editing.as_ref());
        let mut out: Vec<DiffAnnotation> = self
            .review
            .read(cx)
            .comments
            .iter()
            .filter(|c| showing(&c.path) && editing != Some(&c.id))
            .map(|c| {
                let position = DiffLinePosition::new(c.path.clone(), kit_side(c.side), c.line as usize);
                DiffAnnotation::line(ElementId::Name(c.id.to_string().into()), position)
            })
            .collect();
        if let Some(draft) = self.draft.as_ref().filter(|draft| showing(draft.position.path())) {
            out.push(DiffAnnotation::line(ElementId::Name("draft".into()), draft.position.clone()));
        }
        out
    }

    fn diff_view(&self, state: &Entity<DiffState>, cx: &mut Context<Self>) -> AnyElement {
        let (on_add, for_annotations) = (cx.weak_entity(), cx.weak_entity());
        let comments = self.review.read(cx).comments.clone();
        let draft = self.draft.as_ref().map(|draft| (draft.input.clone(), draft.editing.is_some()));
        Diff::new(state)
            .line_number(true)
            .syntax_highlight(true)
            .annotations(self.annotations(cx))
            .on_add_annotation(move |range, window, cx| {
                let position = DiffLinePosition::new(range.path().clone(), range.end_side(), range.end());
                let _ = on_add.update(cx, |t, cx| t.start_comment(position, window, cx));
            })
            .render_annotation(move |annotation, _, cx| {
                let id = annotation.id().clone();
                if id == ElementId::Name("draft".into()) {
                    let Some((input, editing)) = &draft else { return div().into_any_element() };
                    return draft_card(input, *editing, for_annotations.clone(), cx);
                }
                let text = comments.iter().find(|c| ElementId::Name(c.id.to_string().into()) == id);
                let Some(comment) = text.cloned() else { return div().into_any_element() };
                comment_card(comment, for_annotations.clone(), cx)
            })
            .into_any_element()
    }
}

fn placeholder(text: &'static str, color: Hsla) -> AnyElement {
    div().size_full().flex().items_center().justify_center().text_color(color).child(text).into_any_element()
}

impl Render for FileDiff {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let state = self.state.clone().filter(|_| self.file(cx).is_some());
        let loaded = self.review.read(cx).files.is_some();
        let content = match state {
            Some(state) => self.diff_view(&state, cx),
            None if loaded => placeholder("This file has no changes now", muted),
            None => placeholder("Loading…", muted),
        };
        div().track_focus(&self.focus).size_full().child(content)
    }
}
