//! Leaving review comments on a diff, and sending them to the agent. The comments are client-core's
//! copy of the hub's, less what's being deleted here.

use std::fmt::Write as _;

use gpui_kit::component::diff::{DiffLinePosition, DiffLineRange, DiffSide as KitSide};
use gpui_kit::{Context, Window};
use protocol::model::{CommentId, DiffComment, DiffSide};

use client_core::records::Record;

use super::{DiffEvent, DiffPanel, kit_side};

fn our_side(side: KitSide) -> DiffSide {
    match side {
        KitSide::Original => DiffSide::Old,
        KitSide::Modified => DiffSide::New,
    }
}

/// What the agent reads for a set of comments. It says which side matters, because a comment on a
/// removed line refers to a line that's no longer in the file.
pub fn comments_prompt(comments: &[DiffComment]) -> String {
    let mut prompt = String::from("Please address these review comments on the current diff:\n");
    for comment in comments {
        let _ = write!(prompt, "\n- {}", comment_line(comment));
    }
    prompt
}

/// One comment as the agent reads it: where it was left, and what it says.
pub fn comment_line(comment: &DiffComment) -> String {
    let side = match comment.side {
        DiffSide::Old => " (a removed line)",
        DiffSide::New | DiffSide::Unknown => "",
    };
    format!("`{}:{}`{side}: {}", comment.path, comment.line, comment.text)
}

impl DiffPanel {
    /// The comments client-core holds for this workspace.
    pub fn set_comments(&mut self, held: Vec<DiffComment>, cx: &mut Context<Self>) {
        self.deleting.retain(|id| held.iter().any(|c| &c.id == id));
        self.held = held;
        self.show_comments(cx);
    }

    /// What client-core holds, less what's being deleted here.
    fn show_comments(&mut self, cx: &mut Context<Self>) {
        self.comments = self.held.iter().filter(|c| !self.deleting.contains(&c.id)).cloned().collect();
        cx.notify();
    }

    /// A delete failed, so those comments show again.
    fn kept(&mut self, ids: &[CommentId], cx: &mut Context<Self>) {
        for id in ids {
            self.deleting.remove(id);
        }
        self.show_comments(cx);
    }

    pub fn start_comment(&mut self, position: DiffLinePosition, window: &mut Window, cx: &mut Context<Self>) {
        self.write(position, None, String::new(), window, cx);
    }

    /// Opens a comment for editing where it was left, in place of how it shows.
    pub fn edit_comment(&mut self, id: &CommentId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(comment) = self.comments.iter().find(|c| &c.id == id).cloned() else { return };
        let position = DiffLinePosition::new(comment.path, kit_side(comment.side), comment.line as usize);
        self.write(position, Some(comment.id), comment.text, window, cx);
    }

    /// Opens the box for writing at `position`, with `text` in it, for a new comment or the one being edited.
    fn write(
        &mut self,
        position: DiffLinePosition,
        editing: Option<CommentId>,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.draft = Some(position);
        self.editing = editing;
        self.select_draft_line(cx);
        let end = text.len();
        self.draft_input.update(cx, |input, cx| {
            input.set_value(text, window, cx);
            input.set_selected_range(end..end, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub fn cancel_draft(&mut self, cx: &mut Context<Self>) {
        self.draft = None;
        self.editing = None;
        self.select_draft_line(cx);
        cx.notify();
    }

    /// Puts a comment in the chat's composer, for the agent with whatever else is said there. It stays on the diff
    /// until it's deleted, like any other.
    pub fn add_to_chat(&self, id: &CommentId, cx: &mut Context<Self>) {
        let Some(comment) = self.comments.iter().find(|c| &c.id == id) else { return };
        cx.emit(DiffEvent::AddToChat(comment_line(comment)));
    }

    /// Selects the line being commented on, when its file is the one showing, so it stays marked with its add button
    /// showing. Clears the selection once there's no comment being written.
    pub(super) fn select_draft_line(&self, cx: &mut Context<Self>) {
        let Some(state) = &self.state else { return };
        let showing = self.draft.as_ref().filter(|draft| self.open.as_ref() == Some(draft.path()));
        let line =
            showing.map(|draft| DiffLineRange::new(draft.path().clone(), draft.side(), draft.line(), draft.line()));
        if line.is_some() || self.draft.is_none() {
            state.update(cx, |s, cx| s.set_selected_lines(line, cx));
        }
    }

    pub fn save_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(position) = self.draft.clone() else { return };
        let text = self.draft_input.read(cx).text().to_string();
        if text.trim().is_empty() {
            return;
        }
        self.draft = None;
        let editing = self.editing.take();
        self.select_draft_line(cx);
        self.draft_input.update(cx, |input, cx| input.set_value("", window, cx));
        let id = self.workspace_id.clone();
        let saving = self.client.start(move |client| match &editing {
            Some(comment) => client.comment_edit(comment, &text),
            None => client.comment_add(
                &id,
                position.path(),
                our_side(position.side()),
                u32::try_from(position.line()).unwrap_or(u32::MAX),
                &text,
                Some(client_core::new_request_id()),
            ),
        });
        cx.spawn(async move |this, cx| {
            let added = saving.recv().await;
            let _ = this.update(cx, |t, cx| {
                match added {
                    // Shown once client-core's copy has it, like any other change.
                    Ok(comment) => t.records.take_reply(Record::Comment(comment)),
                    Err(e) => cx.emit(DiffEvent::Error(format!("couldn't save that comment: {e}"))),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn delete_comment(&mut self, id: CommentId, cx: &mut Context<Self>) {
        self.deleting.insert(id.clone());
        self.show_comments(cx);
        let target = id.clone();
        let deleting = self.client.start(move |client| client.comment_delete(&target));
        cx.spawn(async move |this, cx| {
            if let Err(e) = deleting.recv().await {
                let _ = this.update(cx, |t, cx| {
                    t.kept(&[id], cx);
                    cx.emit(DiffEvent::Error(format!("couldn't delete that comment: {e}")));
                });
            }
        })
        .detach();
    }

    /// Hands every comment to the agent, and clears them.
    pub fn send_comments(&mut self, cx: &mut Context<Self>) {
        if self.comments.is_empty() {
            return;
        }
        let sent = self.comments.clone();
        self.deleting.extend(sent.iter().map(|c| c.id.clone()));
        self.show_comments(cx);
        cx.emit(DiffEvent::SendToAgent(comments_prompt(&sent)));
        let ids: Vec<CommentId> = sent.iter().map(|c| c.id.clone()).collect();
        let clearing = self.client.start(move |client| {
            for comment in &sent {
                client.comment_delete(&comment.id)?;
            }
            Ok(())
        });
        cx.spawn(async move |this, cx| {
            if let Err(e) = clearing.recv().await {
                // Those already deleted go when client-core hears so, and the rest show again.
                let _ = this.update(cx, |t, cx| {
                    t.kept(&ids, cx);
                    cx.emit(DiffEvent::Error(format!("couldn't clear sent comments: {e}")));
                });
            }
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::comments_prompt;
    use protocol::model::{DiffComment, DiffSide};

    fn comment(path: &str, side: DiffSide, line: u32, text: &str) -> DiffComment {
        DiffComment {
            id: "c".into(),
            workspace_id: "w".into(),
            path: path.into(),
            side,
            line,
            text: text.into(),
            created_at_ms: 0,
            rev: protocol::model::Rev::default(),
        }
    }

    #[test]
    fn the_prompt_names_each_line_and_says_when_it_was_removed() {
        let prompt = comments_prompt(&[
            comment("src/a.rs", DiffSide::New, 12, "Rename this"),
            comment("src/b.rs", DiffSide::Old, 3, "Why remove this?"),
        ]);
        assert!(prompt.contains("`src/a.rs:12`: Rename this"), "{prompt}");
        assert!(prompt.contains("`src/b.rs:3` (a removed line): Why remove this?"), "{prompt}");
    }
}
