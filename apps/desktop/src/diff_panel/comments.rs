//! Leaving review comments on a diff, and sending them to the agent. The comments are client-core's
//! copy of the hub's, less what's being deleted here.

use std::fmt::Write as _;

use gpui_kit::component::diff::{DiffLinePosition, DiffSide as KitSide};
use gpui_kit::{AppContext, Context, Window};
use protocol::model::{CommentId, DiffComment, DiffSide};

use super::{DiffEvent, DiffPanel};

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
        let side = match comment.side {
            DiffSide::Old => " (a removed line)",
            DiffSide::New | DiffSide::Unknown => "",
        };
        let _ = write!(prompt, "\n- `{}:{}`{side}: {}", comment.path, comment.line, comment.text);
    }
    prompt
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
        self.draft = Some(position);
        self.draft_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub fn cancel_draft(&mut self, cx: &mut Context<Self>) {
        self.draft = None;
        cx.notify();
    }

    pub fn save_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(position) = self.draft.clone() else { return };
        let text = self.draft_input.read(cx).text().to_string();
        if text.trim().is_empty() {
            return;
        }
        self.draft = None;
        self.draft_input.update(cx, |input, cx| input.set_value("", window, cx));
        let (client, id) = (self.client.clone(), self.workspace_id.clone());
        cx.spawn(async move |this, cx| {
            let added = cx
                .background_spawn(async move {
                    client.comment_add(
                        &id,
                        position.path(),
                        our_side(position.side()),
                        u32::try_from(position.line()).unwrap_or(u32::MAX),
                        &text,
                        Some(client_core::new_request_id()),
                    )
                })
                .await;
            let _ = this.update(cx, |_, cx| {
                match added {
                    // Shown once client-core's copy has it, like any other change.
                    Ok(comment) => cx.emit(DiffEvent::Replied(comment)),
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
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            let target = id.clone();
            let deleted = cx.background_spawn(async move { client.comment_delete(&target) }).await;
            if let Err(e) = deleted {
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
        let (client, ids): (_, Vec<CommentId>) = (self.client.clone(), sent.iter().map(|c| c.id.clone()).collect());
        cx.spawn(async move |this, cx| {
            let cleared = cx
                .background_spawn(async move {
                    for comment in &sent {
                        client.comment_delete(&comment.id)?;
                    }
                    Ok::<_, client_core::ClientError>(())
                })
                .await;
            if let Err(e) = cleared {
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
