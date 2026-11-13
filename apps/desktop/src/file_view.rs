//! A file as it is in the worktree, as the workspace's file tab shows it when it's opened from All files: its lines,
//! numbered and highlighted for its language, in a list that scrolls both ways, or its image, fitted to the tab, with
//! what kind it is and how big. Which file is the pane's to say, since the file tab is the pane's.

use client_core::Client;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, FocusHandle, Focusable, InteractiveElement as _, IntoElement,
    ListHorizontalSizingBehavior, ParentElement as _, Pixels, Render, SharedString, Styled as _, StyledText,
    Subscription, UniformListScrollHandle, Window, div, px, uniform_list,
};
use look::Typeset as _;
use protocol::model::WorkspaceId;

use crate::file_header::file_header;
use crate::file_image::{FileImage, Placed, fitted};
pub(crate) use lines::{Line, Lines, TAB};
pub(crate) use reading::{HIGHLIGHT_UP_TO, Read, Unshown, hash, what_it_gives};

mod lines;
mod reading;

/// The height of a line of a file, and of a line of a diff, so the two read alike.
pub(crate) const LINE_HEIGHT: Pixels = px(20.);
/// The bar with the file's path, as tall as the rows of tabs above it, so the buttons an image's comparison has there
/// have room around them.
pub(crate) const HEADER_HEIGHT: Pixels = px(40.);

/// What the file view has to show for its file.
enum Contents {
    Loading,
    Lines(Lines),
    Image(FileImage),
    Instead(Unshown),
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
    /// A hash of the bytes what's shown was made from, its lines or its image, so a file read again that hasn't changed
    /// is left as it is.
    shown: Option<u64>,
    _releasing: Subscription,
}

impl Focusable for FileView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl FileView {
    pub fn new(client: Client, workspace_id: WorkspaceId, cx: &mut Context<Self>) -> Self {
        let releasing = cx.on_release(|t, cx| {
            if let Contents::Image(image) = &t.contents {
                image.release(cx);
            }
        });
        Self {
            client,
            workspace_id,
            path: None,
            contents: Contents::Loading,
            scroll: UniformListScrollHandle::new(),
            focus: cx.focus_handle(),
            reads: 0,
            shown: None,
            _releasing: releasing,
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
        let Contents::Instead(Unshown::Failed(message)) = &self.contents else { return None };
        Some(message.clone())
    }

    /// The lines it shows, joined, or `None` while it shows something in their place.
    #[cfg(feature = "testing")]
    pub fn shown_text(&self) -> Option<String> {
        let Contents::Lines(lines) = &self.contents else { return None };
        Some(lines.lines.iter().map(|line| line.text.as_ref()).collect::<Vec<_>>().join("\n"))
    }

    /// What it says of the image it shows, or `None` while it shows something in its place.
    #[cfg(feature = "testing")]
    pub fn shown_image(&self) -> Option<String> {
        let Contents::Image(image) = &self.contents else { return None };
        Some(image.describe())
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

    /// Shows `contents`, made from bytes with the hash `shown`, if `read` is the newest read, and says whether it was.
    fn set_contents(&mut self, read: u64, contents: Contents, shown: Option<u64>, cx: &mut Context<Self>) -> bool {
        if read != self.reads {
            return false;
        }
        if let Contents::Image(image) = std::mem::replace(&mut self.contents, contents) {
            image.release(cx);
        }
        self.shown = shown;
        cx.notify();
        true
    }

    fn draw_lines(&self, range: std::ops::Range<usize>, cx: &App) -> Vec<AnyElement> {
        let colors = look::colors(cx);
        let Contents::Lines(lines) = &self.contents else { return Vec::new() };
        let numbers = numbers_width(lines.lines.len());
        let foreground = colors.text;
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
                            .text_color(colors.text_faint)
                            .child((index + 1).to_string()),
                    )
                    .child(div().flex_none().pr_4().whitespace_nowrap().text_color(foreground).child(text));
                Some(row.into_any_element())
            })
            .collect()
    }

    /// Its path, and for an image, what kind it is and how big, at the other end.
    fn header(&self, cx: &App) -> Option<AnyElement> {
        let path = self.path.as_ref()?;
        let about = match &self.contents {
            Contents::Image(image) => Some(
                div().flex_none().text_color(look::colors(cx).text_muted).child(image.describe()).into_any_element(),
            ),
            _ => None,
        };
        Some(file_header(path, None, about, cx))
    }
}

/// Wide enough for the biggest line number, with room after it.
#[expect(clippy::cast_precision_loss, reason = "a line number has a handful of digits")]
pub(crate) fn numbers_width(lines: usize) -> Pixels {
    let digits = lines.max(1).to_string().len().max(3);
    px(digits as f32 * 8. + 20.)
}

impl Render for FileView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match &self.contents {
            Contents::Loading => placeholder("Loading…".into(), cx),
            Contents::Image(image) => picture(image),
            Contents::Instead(why) => placeholder(why.said().into(), cx),
            Contents::Lines(lines) => {
                uniform_list("file-lines", lines.lines.len(), cx.processor(|t, range, _, cx| t.draw_lines(range, cx)))
                    .track_scroll(&self.scroll)
                    .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
                    .with_width_from_item(Some(lines.widest))
                    .flex_1()
                    .w_full()
                    .py_1()
                    .typeset(look::Type::Code)
                    .into_any_element()
            }
        };
        v_flex().track_focus(&self.focus).size_full().children(self.header(cx)).child(body)
    }
}

/// The image, as big as it is, or smaller to fit the tab, in its middle.
fn picture(image: &FileImage) -> AnyElement {
    let (image, covers) = (image.clone(), (image.width, image.height));
    let room = fitted(covers, Placed::Middle, move |scale, cx| image.whole(scale, cx));
    div().flex_1().w_full().min_h_0().p_6().child(room.size_full()).into_any_element()
}

/// A line in the middle of the room, muted, saying what's there or why nothing is.
pub(crate) fn placeholder(text: SharedString, cx: &App) -> AnyElement {
    div()
        .flex_1()
        .w_full()
        .px_6()
        .flex()
        .items_center()
        .justify_center()
        .text_center()
        .text_color(look::colors(cx).text_muted)
        .child(text)
        .into_any_element()
}
