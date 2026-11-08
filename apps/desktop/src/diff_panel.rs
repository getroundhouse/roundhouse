//! The diff against the workspace's base, on the kit's `Diff`, with review comments the hub keeps.

use std::collections::HashSet;

use client_core::{Client, RecordsFeed};
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
mod comments;

pub use comments::comments_prompt;

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

pub struct DiffPanel {
    client: Client,
    /// Client-core's copy of the hub's records, which replies go into.
    records: RecordsFeed,
    workspace_id: WorkspaceId,
    pub state: Option<Entity<DiffState>>,
    pub empty: bool,
    pub truncated: bool,
    /// What's shown: client-core's copy, less what's being deleted here.
    pub comments: Vec<DiffComment>,
    /// Client-core's copy.
    held: Vec<DiffComment>,
    /// Deleted here, and still in client-core's copy until the feed says they've gone.
    deleting: HashSet<CommentId>,
    /// The line being commented on, if any.
    pub draft: Option<DiffLinePosition>,
    pub draft_input: Entity<TextareaState>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<DiffEvent> for DiffPanel {}

impl DiffPanel {
    pub fn new(
        client: Client,
        records: RecordsFeed,
        workspace_id: WorkspaceId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
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
            records,
            workspace_id,
            state: None,
            empty: false,
            truncated: false,
            comments: Vec::new(),
            held: Vec::new(),
            deleting: HashSet::new(),
            draft: None,
            draft_input,
            _subscriptions: vec![on_enter],
        };
        this.refresh(cx);
        this
    }

    /// Carries on on a new connection to the hub, and loads the diff again on it.
    pub fn reconnect(&mut self, client: &Client, cx: &mut Context<Self>) {
        self.client = client.clone();
        self.refresh(cx);
    }

    /// Loads the diff again. Keeps showing the old one until the new one is ready.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let (client, id) = (self.client.clone(), self.workspace_id.clone());
        cx.spawn(async move |this, cx| {
            let loaded = cx.background_spawn(async move { client.workspace_diff(&id) }).await;
            let diff = match loaded {
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
