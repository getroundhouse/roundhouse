//! The review comments on a workspace's diff, and sending them to the agent. They're client-core's copy of the hub's,
//! less what's being deleted here.

use std::fmt::Write as _;

use gpui_kit::Context;
use protocol::model::{CommentDraft, CommentId, DiffComment, DiffSide, Record};

use super::{Review, ReviewEvent};

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

impl Review {
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

    /// Saves the comment being written, as a new one on its line, or as the text of the one it changes, once `kept`
    /// answers. It shows once client-core's copy has it, like any other change, and says whether it was saved, giving
    /// it back if it wasn't.
    pub fn save_comment(&mut self, saving: CommentDraft, kept: flume::Receiver<()>, cx: &mut Context<Self>) {
        let (id, draft) = (self.workspace_id.clone(), saving.clone());
        let saved = self.client.start(move |client| {
            // An answer, whether it's kept or as far as it could get, so a hub that's gone can't hold the save forever.
            let _ = kept.recv();
            match &draft.editing {
                Some(comment) => client.comment_edit(comment, &draft.text),
                None => client.comment_add(
                    &id,
                    &draft.path,
                    draft.side,
                    draft.line,
                    &draft.text,
                    Some(client_core::new_request_id()),
                ),
            }
        });
        cx.spawn(async move |this, cx| {
            let saved = saved.recv().await;
            let _ = this.update(cx, |t, cx| {
                match saved {
                    Ok(comment) => {
                        t.records.take_reply(Record::Comment(comment));
                        cx.emit(ReviewEvent::Saved);
                    }
                    Err(e) => {
                        cx.emit(ReviewEvent::Error(format!("couldn't save that comment: {e}")));
                        cx.emit(ReviewEvent::NotSaved(saving));
                    }
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
                    cx.emit(ReviewEvent::Error(format!("couldn't delete that comment: {e}")));
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
        cx.emit(ReviewEvent::SendToAgent(comments_prompt(&sent)));
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
                    cx.emit(ReviewEvent::Error(format!("couldn't clear sent comments: {e}")));
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
