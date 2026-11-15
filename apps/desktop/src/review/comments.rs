//! The review comments on a workspace's diff, which the agent reads with the worker's tools. They're client-core's
//! copy of the hub's, less what's being deleted here.

use gpui_kit::Context;
use protocol::model::{CommentDraft, CommentId, DiffComment};

use super::{Review, ReviewEvent};

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

    /// A delete failed, so the comment shows again.
    fn kept(&mut self, id: &CommentId, cx: &mut Context<Self>) {
        self.deleting.remove(id);
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
            let _ = this.update(cx, |_, cx| {
                match saved {
                    Ok(_) => cx.emit(ReviewEvent::Saved),
                    Err(e) => {
                        cx.emit(ReviewEvent::Error(client_core::refusals::couldnt("save that comment", &e)));
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
                    t.kept(&id, cx);
                    cx.emit(ReviewEvent::Error(client_core::refusals::couldnt("delete that comment", &e)));
                });
            }
        })
        .detach();
    }
}
