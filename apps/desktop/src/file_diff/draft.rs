//! The comment being written on a line of the diff, or a saved one being edited, in a box below the line. The box is
//! made when the comment is started, and goes once it's saved or cancelled.

use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::{AppContext as _, Context, Entity, SharedString, Subscription, Window};
use protocol::model::{CommentId, DiffSide};

use super::{FileDiff, FileDiffEvent};
use crate::review::{Saving, comment_line};

/// A line of a file's diff, on one side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineAt {
    pub path: SharedString,
    pub side: DiffSide,
    pub line: u32,
}

pub struct Draft {
    /// The line it's on.
    pub position: LineAt,
    /// The saved comment it edits, rather than adding one.
    pub editing: Option<CommentId>,
    pub input: Entity<TextareaState>,
    _events: Subscription,
}

impl FileDiff {
    pub fn start_comment(&mut self, position: LineAt, window: &mut Window, cx: &mut Context<Self>) {
        self.write(position, None, String::new(), window, cx);
    }

    /// Opens a comment for editing where it was left, in place of how it shows.
    pub fn edit_comment(&mut self, id: &CommentId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(comment) = self.review.read(cx).comment(id).cloned() else { return };
        let position = LineAt { path: comment.path.into(), side: comment.side, line: comment.line };
        self.write(position, Some(comment.id), comment.text, window, cx);
    }

    /// Opens the box for writing at `position`, with `text` in it, for a new comment or the one being edited. It
    /// takes the place of any comment being written.
    fn write(
        &mut self,
        position: LineAt,
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
        self.lay_out_again(cx);
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
            None => Saving::New { path: draft.position.path, side: draft.position.side, line: draft.position.line },
        };
        self.review.update(cx, |r, cx| r.save_comment(saving, text, cx));
    }

    /// Takes the box away, and the focus it had with it, which comes back to the diff so the window's keys still
    /// work.
    fn put_away_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<Draft> {
        let draft = self.draft.take();
        window.focus(&self.focus, cx);
        self.lay_out_again(cx);
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
}
