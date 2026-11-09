//! The diff against the workspace's base, and the review comments the hub keeps on it. It loads every changed file,
//! which the sidebar on the right lists, and shows one at a time, on the kit's `Diff`, in the workspace's diff tab.

use std::collections::HashSet;

use client_core::{Client, RecordsFeed};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::diff::{
    Diff, DiffAnnotation, DiffFile, DiffLinePosition, DiffMode, DiffSide as KitSide, DiffState,
};
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::{
    AnyElement, AppContext, Context, ElementId, Entity, EventEmitter, HighlightStyle, Hsla, IntoElement, ParentElement,
    Render, SharedString, Styled, Subscription, Window, div,
};
use protocol::model::{CommentId, DiffComment, DiffSide, WorkspaceId};

use annotations::{comment_card, draft_card};

mod annotations;
mod comments;

pub use comments::{comment_line, comments_prompt};

#[derive(Clone, Debug)]
pub enum DiffEvent {
    Error(String),
    SendToAgent(String),
    /// Put this in the chat's composer.
    AddToChat(String),
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
    /// Every changed file, in order of path, or `None` until the diff has loaded.
    pub files: Option<Vec<DiffFile>>,
    pub truncated: bool,
    /// The file the diff tab shows.
    pub open: Option<SharedString>,
    /// The kit's view of the open file, once there's been one.
    pub state: Option<Entity<DiffState>>,
    /// What's shown: client-core's copy, less what's being deleted here.
    pub comments: Vec<DiffComment>,
    /// Client-core's copy.
    held: Vec<DiffComment>,
    /// Deleted here, and still in client-core's copy until the feed says they've gone.
    deleting: HashSet<CommentId>,
    /// The line being commented on, if any.
    pub draft: Option<DiffLinePosition>,
    /// The comment being written is this one, edited, rather than a new one.
    editing: Option<CommentId>,
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
            TextareaState::new(window, cx)
                .auto_grow(2, 6)
                .submit_on_enter(true)
                .placeholder("Add a comment for the agent")
        });
        let on_enter = cx.subscribe_in(&draft_input, window, |this, _, event: &InputEvent, window, cx| match event {
            InputEvent::PressEnter { shift: false, .. } => this.save_draft(window, cx),
            // So Comment can say whether there's anything to save.
            InputEvent::Change => cx.notify(),
            _ => {}
        });
        let mut this = Self {
            client,
            records,
            workspace_id,
            files: None,
            truncated: false,
            open: None,
            state: None,
            comments: Vec::new(),
            held: Vec::new(),
            deleting: HashSet::new(),
            draft: None,
            editing: None,
            draft_input,
            _subscriptions: vec![on_enter],
        };
        this.refresh(cx);
        this
    }

    /// Carries on on a new connection to the hub, keeping a comment that's being written. The diff is loaded
    /// again on it with the next [`Self::refresh`].
    pub fn reconnect(&mut self, client: &Client) {
        self.client = client.clone();
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
            let truncated = diff.truncated;
            let parsed = cx
                .background_spawn(async move {
                    let mut files = DiffFile::parse(&diff.patch).map_err(|e| e.to_string())?;
                    files.sort_by(|a, b| a.path().cmp(b.path()));
                    Ok::<_, String>(files)
                })
                .await;
            let _ = this.update(cx, |t, cx| {
                t.truncated = truncated;
                match parsed {
                    Ok(files) => {
                        t.files = Some(files);
                        t.show_open(cx);
                    }
                    Err(e) => cx.emit(DiffEvent::Error(format!("couldn't read the diff: {e}"))),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Whether the diff has loaded, with nothing changed.
    pub fn has_no_changes(&self) -> bool {
        self.files.as_ref().is_some_and(Vec::is_empty)
    }

    /// Shows a file in the diff tab, in place of the one it showed. A comment being written on that one is kept, and
    /// shows again with it.
    pub fn open(&mut self, path: SharedString, cx: &mut Context<Self>) {
        if self.open.as_ref() != Some(&path) {
            self.open = Some(path);
            self.show_open(cx);
        }
        cx.notify();
    }

    /// Closes the diff tab. A comment being written in it is kept, and shows again when its file does.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.open = None;
        cx.notify();
    }

    /// The open file, as the diff last loaded has it. `None` once it no longer changes anything.
    pub fn open_file(&self) -> Option<&DiffFile> {
        let open = self.open.as_ref()?;
        self.files.as_ref()?.iter().find(|file| file.path() == open)
    }

    /// Gives the kit the open file's diff as it is now.
    fn show_open(&mut self, cx: &mut Context<Self>) {
        let Some(file) = self.open_file().cloned() else { return };
        if let Some(state) = &self.state {
            state.update(cx, |s, cx| s.set_files([file], cx));
            // New files clear the selection, which is what marks the line being commented on.
            self.select_draft_line(cx);
        } else {
            let state = cx.new(|cx| {
                DiffState::new([file], cx).with_mode(DiffMode::Unified).with_syntax_highlighter(|language, text| {
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
        let showing = |path: &str| self.open.as_ref().is_some_and(|open| open.as_ref() == path);
        let mut out: Vec<DiffAnnotation> = self
            .comments
            .iter()
            .filter(|c| showing(&c.path) && self.editing.as_ref() != Some(&c.id))
            .map(|c| {
                let position = DiffLinePosition::new(c.path.clone(), kit_side(c.side), c.line as usize);
                DiffAnnotation::line(ElementId::Name(c.id.to_string().into()), position)
            })
            .collect();
        if let Some(position) = self.draft.as_ref().filter(|position| showing(position.path())) {
            out.push(DiffAnnotation::line(ElementId::Name("draft".into()), position.clone()));
        }
        out
    }
}

impl DiffPanel {
    fn diff_view(&self, state: &Entity<DiffState>, cx: &mut Context<Self>) -> AnyElement {
        let (on_add, for_annotations) = (cx.weak_entity(), cx.weak_entity());
        let comments = self.comments.clone();
        let (draft_input, editing) = (self.draft_input.clone(), self.editing.is_some());
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
                    return draft_card(&draft_input, editing, for_annotations.clone(), cx);
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

impl Render for DiffPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let state = self.state.clone().filter(|_| self.open_file().is_some());
        let content = match (state, &self.files) {
            (Some(state), _) => self.diff_view(&state, cx),
            (None, Some(_)) => placeholder("This file has no changes now", muted),
            (None, None) => placeholder("Loading…", muted),
        };
        div().size_full().child(content)
    }
}
