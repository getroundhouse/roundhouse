//! A file as it is in the worktree, as the workspace's file tab shows it when it's opened from All files: its lines,
//! numbered and highlighted for its language, in a list that scrolls both ways. Which file is the pane's to say, since
//! the file tab is the pane's.

use std::hash::{DefaultHasher, Hash as _, Hasher as _};

use client_core::{Client, ClientError};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, FocusHandle, Focusable, Hsla, InteractiveElement as _, IntoElement,
    ListHorizontalSizingBehavior, ParentElement as _, Pixels, Render, SharedString, Styled as _, StyledText,
    UniformListScrollHandle, Window, div, px, uniform_list,
};
use protocol::attachment::describe_size;
use protocol::client::ErrorCode;
use protocol::model::{FileContents, WorkspaceId};

use lines::Lines;

mod lines;

/// The same height as a line of the diff, so the two read alike.
const LINE_HEIGHT: Pixels = px(20.);
const HEADER_HEIGHT: Pixels = px(32.);
/// Above this, a file is highlighted only after it's shown plain, since highlighting it takes a noticeable moment.
const QUICK_BYTES: usize = 64 * 1024;
/// Above this, a file is left plain, since highlighting it would take seconds.
const HIGHLIGHT_UP_TO: usize = 1024 * 1024;

/// What the file view has to show for its file.
enum Contents {
    Loading,
    Lines(Lines),
    Empty,
    Binary,
    TooBig(u64),
    /// It was there when the files were listed, and isn't now.
    Gone,
    /// The hub is older than the app, and can't read files.
    Unsupported,
    Failed(String),
}

pub struct FileView {
    client: Client,
    workspace_id: WorkspaceId,
    /// The file the file tab shows, while it shows one.
    pub path: Option<SharedString>,
    contents: Contents,
    scroll: UniformListScrollHandle,
    /// Where keys go while the file tab shows a file, so the window's keys reach the app.
    focus: FocusHandle,
    /// How many times a file has been read, so only the newest answer is shown.
    reads: u64,
    /// A hash of the text the lines shown were made from, so a file read again that hasn't changed is left as it is.
    shown_text: Option<u64>,
}

impl Focusable for FileView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl FileView {
    pub fn new(client: Client, workspace_id: WorkspaceId, cx: &mut Context<Self>) -> Self {
        Self {
            client,
            workspace_id,
            path: None,
            contents: Contents::Loading,
            scroll: UniformListScrollHandle::new(),
            focus: cx.focus_handle(),
            reads: 0,
            shown_text: None,
        }
    }

    /// Carries on on a new connection to the hub. A read on the old one is dropped, and the file is read again on this
    /// one with the next [`Self::refresh`].
    pub fn reconnect(&mut self, client: &Client) {
        self.client = client.clone();
        self.reads += 1;
    }

    /// Why the file couldn't be read, if that's what shows.
    #[cfg(feature = "testing")]
    pub fn failure(&self) -> Option<String> {
        let Contents::Failed(message) = &self.contents else { return None };
        Some(message.clone())
    }

    /// The lines it shows, joined, or `None` while it shows something in their place.
    #[cfg(feature = "testing")]
    pub fn shown_text(&self) -> Option<String> {
        let Contents::Lines(lines) = &self.contents else { return None };
        Some(lines.lines.iter().map(|line| line.text.as_ref()).collect::<Vec<_>>().join("\n"))
    }

    /// Shows a file from its first line, in place of the one it showed, or none, as the file tab says.
    pub fn show(&mut self, path: Option<SharedString>, cx: &mut Context<Self>) {
        if self.path != path {
            self.path = path;
            // So what's being read of the file before is dropped when it comes, and isn't kept for nothing.
            self.reads += 1;
            self.set_contents(self.reads, Contents::Loading, None, cx);
            self.scroll = UniformListScrollHandle::new();
            self.read(cx);
        }
        cx.notify();
    }

    /// Reads the file again, keeping what's shown, and where it's scrolled to, until it's read.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.read(cx);
    }

    fn read(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.path.clone() else { return };
        self.reads += 1;
        let (read, client, id, shown) = (self.reads, self.client.clone(), self.workspace_id.clone(), self.shown_text);
        cx.spawn(async move |this, cx| {
            let file = path.to_string();
            let contents = cx.background_spawn(async move { client.workspace_file_read(&id, &file) }).await;
            // Lost with the connection, so what's shown stays, and the file is read again once the pane catches up on
            // the next one.
            if matches!(contents, Err(ClientError::Disconnected)) {
                return;
            }
            let text = match contents {
                Ok(FileContents::Text { text }) if text.is_empty() => Err(Contents::Empty),
                Ok(FileContents::Text { text }) => Ok(text),
                // An image shows as a file that isn't text, as it did before images were sent.
                Ok(FileContents::Binary | FileContents::Image { .. } | FileContents::Unknown) => Err(Contents::Binary),
                Ok(FileContents::TooBig { bytes }) => Err(Contents::TooBig(bytes)),
                Err(e) if e.is_unsupported() => Err(Contents::Unsupported),
                Err(ClientError::Api(e)) if e.code == ErrorCode::NotFound => Err(Contents::Gone),
                Err(e) => Err(Contents::Failed(e.to_string())),
            };
            let text = match text {
                Ok(text) => text,
                Err(contents) => {
                    let _ = this.update(cx, |t, cx| t.set_contents(read, contents, None, cx));
                    return;
                }
            };
            let (hash, text) = cx.background_spawn(async move { (hash(&text), text) }).await;
            // Read again and the same as it was, so what's shown, colours and all, still is.
            if shown == Some(hash) {
                return;
            }
            let language = (text.len() <= HIGHLIGHT_UP_TO).then(|| look::language(&path).to_string());
            // A big file is shown plain first, the first time, rather than left loading while it's highlighted. Read
            // again, what's shown stays, in its colours, until the new lines are.
            let Some(language) = language else {
                let lines = cx.background_spawn(async move { Lines::new(&text, None) }).await;
                let _ = this.update(cx, |t, cx| t.set_contents(read, Contents::Lines(lines), Some(hash), cx));
                return;
            };
            if text.len() > QUICK_BYTES && shown.is_none() {
                let plain = text.clone();
                let lines = cx.background_spawn(async move { Lines::new(&plain, None) }).await;
                let current = this.update(cx, |t, cx| t.set_contents(read, Contents::Lines(lines), None, cx));
                // Not highlighted for nothing once another file, or another read, has taken its place.
                if !current.unwrap_or(false) {
                    return;
                }
            }
            let lines = cx.background_spawn(async move { Lines::new(&text, Some(&language)) }).await;
            let _ = this.update(cx, |t, cx| t.set_contents(read, Contents::Lines(lines), Some(hash), cx));
        })
        .detach();
    }

    /// Shows `contents`, from the text with the hash `text`, if `read` is the newest read, and says whether it was.
    fn set_contents(&mut self, read: u64, contents: Contents, text: Option<u64>, cx: &mut Context<Self>) -> bool {
        if read != self.reads {
            return false;
        }
        self.contents = contents;
        self.shown_text = text;
        cx.notify();
        true
    }

    fn draw_lines(&self, range: std::ops::Range<usize>, cx: &App) -> Vec<AnyElement> {
        let Contents::Lines(lines) = &self.contents else { return Vec::new() };
        let numbers = numbers_width(lines.lines.len());
        let foreground = cx.theme().foreground;
        range
            .filter_map(|index| {
                let line = lines.lines.get(index)?;
                let text = StyledText::new(line.text.clone()).with_highlights(line.styles.iter().cloned());
                let row = h_flex()
                    .h(LINE_HEIGHT)
                    .child(
                        div()
                            .flex_none()
                            .w(numbers)
                            .pr_4()
                            .text_right()
                            .text_color(look::faint())
                            .child((index + 1).to_string()),
                    )
                    .child(div().flex_none().pr_4().whitespace_nowrap().text_color(foreground).child(text));
                Some(row.into_any_element())
            })
            .collect()
    }

    fn header(&self, cx: &App) -> Option<AnyElement> {
        let path = self.path.clone()?;
        let theme = cx.theme();
        let header = h_flex()
            .flex_none()
            .w_full()
            .h(HEADER_HEIGHT)
            .px_4()
            .border_b_1()
            .border_color(theme.border)
            .text_sm()
            .child(div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(path));
        Some(header.into_any_element())
    }
}

/// Wide enough for the biggest line number, with room after it.
#[expect(clippy::cast_precision_loss, reason = "a line number has a handful of digits")]
fn numbers_width(lines: usize) -> Pixels {
    let digits = lines.max(1).to_string().len().max(3);
    px(digits as f32 * 8. + 20.)
}

impl Render for FileView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (muted, mono, mono_size) = (theme.muted_foreground, theme.mono_font_family.clone(), theme.mono_font_size);
        let body = match &self.contents {
            Contents::Loading => placeholder("Loading…".into(), muted),
            Contents::Empty => placeholder("This file is empty".into(), muted),
            Contents::Binary => placeholder("This file isn't text, so it can't be shown".into(), muted),
            Contents::TooBig(bytes) => {
                placeholder(format!("This file is too big to show, at {}", describe_size(*bytes)).into(), muted)
            }
            Contents::Gone => placeholder("This file isn't there now".into(), muted),
            Contents::Unsupported => placeholder("The hub is too old to show files".into(), muted),
            Contents::Failed(message) => placeholder(format!("Couldn't read the file: {message}").into(), muted),
            Contents::Lines(lines) => {
                uniform_list("file-lines", lines.lines.len(), cx.processor(|t, range, _, cx| t.draw_lines(range, cx)))
                    .track_scroll(&self.scroll)
                    .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
                    .with_width_from_item(Some(lines.widest))
                    .flex_1()
                    .w_full()
                    .py_1()
                    .font_family(mono)
                    .text_size(mono_size)
                    .into_any_element()
            }
        };
        v_flex().track_focus(&self.focus).size_full().children(self.header(cx)).child(body)
    }
}

fn placeholder(text: SharedString, color: Hsla) -> AnyElement {
    div()
        .flex_1()
        .w_full()
        .px_6()
        .flex()
        .items_center()
        .justify_center()
        .text_center()
        .text_color(color)
        .child(text)
        .into_any_element()
}

/// A hash of a file's text, to tell whether it changed since it was shown without keeping a copy of it.
fn hash(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}
