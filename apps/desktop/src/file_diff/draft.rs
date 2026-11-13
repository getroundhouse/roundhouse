//! The comment being written on a line of the diff, or a saved one being edited, in a box below the line. The box is
//! made when the comment is started, and goes once it's saved or cancelled. The hub keeps what's in it, as
//! [`FileDiffEvent::Writing`] asks, so a comment kept there, from before the app last stopped or from another client,
//! opens the box again where it was left.

use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::{
    App, AppContext as _, Context, Entity, FocusHandle, Focusable as _, SharedString, Subscription, Window,
};
use protocol::model::{CommentDraft, CommentId, DiffSide};

use super::{FileDiff, FileDiffEvent};
use crate::review::comment_line;

/// A line of a file's diff, on one side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineAt {
    pub path: SharedString,
    pub side: DiffSide,
    pub line: u32,
}

/// Who puts the box away, and why: whoever's here, cancelling it or saving it, or another client, through the hub.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Away {
    Cancelled,
    Saved,
    Taken,
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
        window.focus(&self.draft_focus(cx), cx);
        cx.emit(FileDiffEvent::Writing { at_once: true });
    }

    /// Opens a comment for editing where it was left, in place of how it shows.
    pub fn edit_comment(&mut self, id: &CommentId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(comment) = self.review.read(cx).comment(id).cloned() else { return };
        let position = LineAt { path: comment.path.into(), side: comment.side, line: comment.line };
        self.write(position, Some(comment.id), comment.text, window, cx);
        window.focus(&self.draft_focus(cx), cx);
        cx.emit(FileDiffEvent::Writing { at_once: true });
    }

    /// The comment being written, as the hub keeps it, or `None` with none.
    pub fn writing(&self, cx: &App) -> Option<CommentDraft> {
        let draft = self.draft.as_ref()?;
        let at = &draft.position;
        Some(CommentDraft {
            path: at.path.to_string(),
            side: at.side,
            line: at.line,
            editing: draft.editing.clone(),
            text: draft.input.read(cx).text().to_string(),
        })
    }

    /// Opens the box where a comment kept on the hub was left, with what was in it, in place of the one being written
    /// here, or takes the box away for `None`. It doesn't take the focus, since whoever's here may be typing elsewhere,
    /// and an open box keeps it, as the same box moves. The hub has it already, so nothing is kept again.
    pub fn take_writing(&mut self, kept: Option<&CommentDraft>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(kept) = kept else {
            self.put_away_draft(Away::Taken, window, cx);
            return;
        };
        let position = LineAt { path: kept.path.clone().into(), side: kept.side, line: kept.line };
        let Some(draft) = self.draft.as_mut() else {
            self.write(position, kept.editing.clone(), kept.text.clone(), window, cx);
            return;
        };
        (draft.position, draft.editing) = (position, kept.editing.clone());
        let input = draft.input.clone();
        if *input.read(cx).text() != kept.text {
            let (text, end) = (kept.text.clone(), kept.text.len());
            input.update(cx, |input, cx| {
                input.set_value(text, window, cx);
                input.set_selected_range(end..end, cx);
            });
        }
        self.lay_out_again(cx);
    }

    fn draft_focus(&self, cx: &App) -> FocusHandle {
        self.draft.as_ref().map_or_else(|| self.focus.clone(), |draft| draft.input.focus_handle(cx))
    }

    /// Opens the box for writing at `position`, with `text` in it, for a new comment or the one being edited. It
    /// takes the place of any comment being written, and leaves the focus where it is.
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
            // So Comment can say whether there's anything to save, and the hub keeps it.
            InputEvent::Change => {
                cx.emit(FileDiffEvent::Writing { at_once: false });
                cx.notify();
            }
            _ => {}
        });
        let end = text.len();
        input.update(cx, |input, cx| {
            input.set_value(text, window, cx);
            input.set_selected_range(end..end, cx);
        });
        self.draft = Some(Draft { position, editing, input, _events: events });
        self.lay_out_again(cx);
    }

    pub fn cancel_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.put_away_draft(Away::Cancelled, window, cx);
    }

    /// Takes the box away and asks for what's in it to be saved. The hub goes on keeping it until the save reaches it,
    /// which lets go of it there, so a save that fails loses nothing.
    pub fn save_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(saving) = self.writing(cx).filter(|writing| !writing.text.trim().is_empty()) else { return };
        self.put_away_draft(Away::Saved, window, cx);
        cx.emit(FileDiffEvent::Save(saving));
    }

    /// Opens the box again with a comment that couldn't be saved, unless another has opened since, such as one started
    /// here or taken from the hub. The hub still keeps it, so nothing is kept again. It takes the focus back only from
    /// the diff, where saving left it.
    pub fn give_back_writing(&mut self, saving: CommentDraft, window: &mut Window, cx: &mut Context<Self>) {
        if self.draft.is_some() {
            return;
        }
        let refocus = self.focus.is_focused(window);
        let position = LineAt { path: saving.path.into(), side: saving.side, line: saving.line };
        self.write(position, saving.editing, saving.text, window, cx);
        if refocus {
            window.focus(&self.draft_focus(cx), cx);
        }
    }

    /// Takes the box away. Put away here, it takes the focus it had with it, which comes back to the diff so the window's
    /// keys still work. Cancelled, the hub is told, and saved, the save tells it. Taken from the hub, which has it
    /// already, the focus moves only if the box had it.
    fn put_away_draft(&mut self, away: Away, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.draft.take() else { return };
        if away != Away::Taken || draft.input.focus_handle(cx).is_focused(window) {
            window.focus(&self.focus, cx);
        }
        if away == Away::Cancelled {
            cx.emit(FileDiffEvent::Writing { at_once: true });
        }
        self.lay_out_again(cx);
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
