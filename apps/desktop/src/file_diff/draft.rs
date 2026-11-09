//! The comment being written on a line of the diff, or a saved one being edited, in a box below the line. The box is
//! made when the comment is started, and goes once it's saved or cancelled.

use gpui_kit::component::diff::{DiffLinePosition, DiffLineRange};
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::{AppContext as _, Context, Entity, Subscription, Window};
use protocol::model::CommentId;

use super::{FileDiff, FileDiffEvent, kit_side, our_side};
use crate::review::{Saving, comment_line};

pub struct Draft {
    /// The line it's on.
    pub position: DiffLinePosition,
    /// The saved comment it edits, rather than adding one.
    pub editing: Option<CommentId>,
    pub input: Entity<TextareaState>,
    _events: Subscription,
}

impl FileDiff {
    pub fn start_comment(&mut self, position: DiffLinePosition, window: &mut Window, cx: &mut Context<Self>) {
        self.write(position, None, String::new(), window, cx);
    }

    /// Opens a comment for editing where it was left, in place of how it shows.
    pub fn edit_comment(&mut self, id: &CommentId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(comment) = self.review.read(cx).comment(id).cloned() else { return };
        let position = DiffLinePosition::new(comment.path, kit_side(comment.side), comment.line as usize);
        self.write(position, Some(comment.id), comment.text, window, cx);
    }

    /// Opens the box for writing at `position`, with `text` in it, for a new comment or the one being edited. It
    /// takes the place of any comment being written.
    fn write(
        &mut self,
        position: DiffLinePosition,
        editing: Option<CommentId>,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(2, 6)
                .submit_on_enter(true)
                .placeholder("Add a comment for the agent")
        });
        let events = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| match event {
            InputEvent::PressEnter { shift: false, .. } => this.save_draft(window, cx),
            // So Comment can say whether there's anything to save.
            InputEvent::Change => cx.notify(),
            _ => {}
        });
        let end = text.len();
        input.update(cx, |input, cx| {
            input.set_value(text, window, cx);
            input.set_selected_range(end..end, cx);
            input.focus(window, cx);
        });
        self.draft = Some(Draft { position, editing, input, _events: events });
        self.select_draft_line(cx);
        cx.notify();
    }

    pub fn cancel_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.put_away_draft(window, cx);
    }

    pub fn save_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.draft.as_ref().map(|draft| draft.input.read(cx).text().to_string());
        let Some(text) = typed.filter(|text| !text.trim().is_empty()) else { return };
        let Some(draft) = self.put_away_draft(window, cx) else { return };
        let saving = match draft.editing {
            Some(id) => Saving::Edit(id),
            None => Saving::New {
                path: draft.position.path().clone(),
                side: our_side(draft.position.side()),
                line: u32::try_from(draft.position.line()).unwrap_or(u32::MAX),
            },
        };
        self.review.update(cx, |r, cx| r.save_comment(saving, text, cx));
    }

    /// Takes the box away, and the focus it had with it, which comes back to the diff so the window's keys still
    /// work.
    fn put_away_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<Draft> {
        let draft = self.draft.take();
        self.select_draft_line(cx);
        window.focus(&self.focus, cx);
        cx.notify();
        draft
    }

    pub fn delete_comment(&self, id: CommentId, cx: &mut Context<Self>) {
        self.review.update(cx, |r, cx| r.delete_comment(id, cx));
    }

    /// Puts a comment in the chat's composer, for the agent with whatever else is said there. It stays on the diff
    /// until it's deleted, like any other.
    pub fn add_to_chat(&self, id: &CommentId, cx: &mut Context<Self>) {
        let Some(line) = self.review.read(cx).comment(id).map(comment_line) else { return };
        cx.emit(FileDiffEvent::AddToChat(line));
    }

    /// Selects the line being commented on, when its file is the one showing, so it stays marked with its add button
    /// showing. Clears the selection once there's no comment being written.
    pub(super) fn select_draft_line(&self, cx: &mut Context<Self>) {
        let Some(state) = &self.state else { return };
        let showing = self.draft.as_ref().filter(|draft| self.path.as_ref() == Some(draft.position.path()));
        let line = showing.map(|draft| {
            let at = &draft.position;
            DiffLineRange::new(at.path().clone(), at.side(), at.line(), at.line())
        });
        if line.is_some() || self.draft.is_none() {
            state.update(cx, |s, cx| s.set_selected_lines(line, cx));
        }
    }
}
