//! The diff against the workspace's base, on the kit's `Diff`, with review comments the hub keeps.

use std::fmt::Write as _;

use client_core::Client;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::diff::{
    Diff, DiffAnnotation, DiffFile, DiffLinePosition, DiffMode, DiffSide as KitSide, DiffState,
};
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext, Context, ElementId, Entity, EventEmitter, HighlightStyle, Hsla, IntoElement, ParentElement,
    Render, Styled, Subscription, Window, div, px,
};
use protocol::model::{CommentId, DiffComment, DiffSide, WorkspaceId};

use annotations::{comment_card, draft_card};

mod annotations;

#[derive(Clone, Debug)]
pub enum DiffEvent {
    Error(String),
    SendToAgent(String),
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

pub struct DiffPanel {
    client: Client,
    workspace_id: WorkspaceId,
    pub state: Option<Entity<DiffState>>,
    pub empty: bool,
    pub truncated: bool,
    pub comments: Vec<DiffComment>,
    /// The line being commented on, if any.
    pub draft: Option<DiffLinePosition>,
    pub draft_input: Entity<TextareaState>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<DiffEvent> for DiffPanel {}

impl DiffPanel {
    pub fn new(client: Client, workspace_id: WorkspaceId, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let draft_input = cx.new(|cx| {
            TextareaState::new(window, cx).auto_grow(2, 6).submit_on_enter(true).placeholder("Leave a comment")
        });
        let on_enter = cx.subscribe_in(&draft_input, window, |this, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { shift: false, .. } = event {
                this.save_draft(window, cx);
            }
        });
        let mut this = Self {
            client,
            workspace_id,
            state: None,
            empty: false,
            truncated: false,
            comments: Vec::new(),
            draft: None,
            draft_input,
            _subscriptions: vec![on_enter],
        };
        this.refresh(cx);
        this
    }

    /// Loads the diff and comments again. Keeps showing the old one until the new one is ready.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let (client, id) = (self.client.clone(), self.workspace_id.clone());
        cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_spawn(async move {
                    let diff = client.workspace_diff(&id)?;
                    let comments = client.comment_list(&id)?;
                    Ok::<_, client_core::ClientError>((diff, comments))
                })
                .await;
            let (diff, comments) = match loaded {
                Ok(loaded) => loaded,
                Err(e) => {
                    let _ = this.update(cx, |_, cx| cx.emit(DiffEvent::Error(format!("couldn't load the diff: {e}"))));
                    return;
                }
            };
            let empty = diff.patch.trim().is_empty();
            let truncated = diff.truncated;
            let parsed =
                cx.background_spawn(async move { DiffFile::parse(&diff.patch).map_err(|e| e.to_string()) }).await;
            let _ = this.update(cx, |t, cx| {
                t.comments = comments;
                t.empty = empty;
                t.truncated = truncated;
                match parsed {
                    Ok(files) => t.show_files(files, cx),
                    Err(e) => cx.emit(DiffEvent::Error(format!("couldn't read the diff: {e}"))),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn show_files(&mut self, files: Vec<DiffFile>, cx: &mut Context<Self>) {
        if let Some(state) = &self.state {
            state.update(cx, |s, cx| s.set_files(files, cx));
        } else {
            let state = cx.new(|cx| {
                DiffState::new(files, cx).with_mode(DiffMode::Unified).with_syntax_highlighter(|language, text| {
                    crate::highlight::highlight(language, text)
                        .into_iter()
                        .map(|span| {
                            let style = HighlightStyle { color: Some(span.style.color), ..Default::default() };
                            (span.range, style)
                        })
                        .collect()
                })
            });
            self.state = Some(state);
        }
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
            let _ = this.update(cx, |t, cx| {
                match added {
                    Ok(comment) => t.comments.push(comment),
                    Err(e) => cx.emit(DiffEvent::Error(format!("couldn't save that comment: {e}"))),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn delete_comment(&mut self, id: CommentId, cx: &mut Context<Self>) {
        self.comments.retain(|c| c.id != id);
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            let deleted = cx.background_spawn(async move { client.comment_delete(&id) }).await;
            if let Err(e) = deleted {
                let _ =
                    this.update(cx, |_, cx| cx.emit(DiffEvent::Error(format!("couldn't delete that comment: {e}"))));
            }
        })
        .detach();
        cx.notify();
    }

    /// Hands every comment to the agent, and clears them.
    pub fn send_comments(&mut self, cx: &mut Context<Self>) {
        if self.comments.is_empty() {
            return;
        }
        let sent = std::mem::take(&mut self.comments);
        cx.emit(DiffEvent::SendToAgent(comments_prompt(&sent)));
        let client = self.client.clone();
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
                let _ =
                    this.update(cx, |_, cx| cx.emit(DiffEvent::Error(format!("couldn't clear sent comments: {e}"))));
            }
        })
        .detach();
        cx.notify();
    }

    fn annotations(&self) -> Vec<DiffAnnotation> {
        let mut out: Vec<DiffAnnotation> = self
            .comments
            .iter()
            .map(|c| {
                let position = DiffLinePosition::new(c.path.clone(), kit_side(c.side), c.line as usize);
                DiffAnnotation::line(ElementId::Name(c.id.to_string().into()), position)
            })
            .collect();
        if let Some(position) = &self.draft {
            out.push(DiffAnnotation::line(ElementId::Name("draft".into()), position.clone()));
        }
        out
    }
}

impl DiffPanel {
    fn diff_view(&self, state: &Entity<DiffState>, cx: &mut Context<Self>) -> AnyElement {
        let (on_add, for_annotations) = (cx.weak_entity(), cx.weak_entity());
        let comments = self.comments.clone();
        let draft_input = self.draft_input.clone();
        Diff::new(state)
            .line_number(true)
            .syntax_highlight(true)
            .annotations(self.annotations())
            .on_add_annotation(move |range, window, cx| {
                let position = DiffLinePosition::new(range.path().clone(), range.end_side(), range.end());
                let _ = on_add.update(cx, |t, cx| t.start_comment(position, window, cx));
            })
            .render_annotation(move |annotation, _, cx| {
                let id = annotation.id().clone();
                if id == ElementId::Name("draft".into()) {
                    return draft_card(&draft_input, for_annotations.clone(), cx);
                }
                let text = comments.iter().find(|c| ElementId::Name(c.id.to_string().into()) == id);
                let Some(comment) = text.cloned() else { return div().into_any_element() };
                comment_card(comment, for_annotations.clone(), cx)
            })
            .into_any_element()
    }

    fn comments_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.comments.len();
        h_flex()
            .flex_none()
            .w_full()
            .h(px(40.))
            .px_3()
            .gap_2()
            .justify_between()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui_kit::FontWeight::MEDIUM)
                    .child(format!("{count} comment{}", if count == 1 { "" } else { "s" })),
            )
            .child(
                Button::new("send-comments")
                    .primary()
                    .small()
                    .label("Send to agent")
                    .on_click(cx.listener(|t, _, _, cx| t.send_comments(cx))),
            )
    }
}

fn placeholder(text: &'static str, color: Hsla) -> AnyElement {
    div().size_full().flex().items_center().justify_center().text_color(color).child(text).into_any_element()
}

impl Render for DiffPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (muted, secondary) = (cx.theme().muted_foreground, cx.theme().secondary);
        let content = if self.empty {
            placeholder("No changes yet", muted)
        } else if let Some(state) = self.state.clone() {
            self.diff_view(&state, cx)
        } else {
            placeholder("Loading…", muted)
        };
        v_flex()
            .size_full()
            .when(self.truncated, |d| {
                d.child(
                    div().flex_none().px_3().py_1().text_xs().bg(secondary).child("Some untracked files aren't shown"),
                )
            })
            .when(!self.comments.is_empty(), |d| d.child(self.comments_bar(cx)))
            .child(div().flex_1().min_h_0().w_full().child(content))
    }
}

#[cfg(test)]
mod tests {
    use super::{DiffComment, DiffSide, comments_prompt};

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
