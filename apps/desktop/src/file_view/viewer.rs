use super::markdown::{MarkdownView, is_markdown};
use super::{lines, reading};
use crate::file_header::file_header;
use crate::file_image::{FileImage, Placed, fitted};
use crate::shared_choice::{Chosen, chosen, switch};
use client_core::Client;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, Focusable, InteractiveElement as _, IntoElement,
    ListHorizontalSizingBehavior, ParentElement as _, Pixels, Render, SharedString, Styled as _, StyledText,
    Subscription, UniformListScrollHandle, Window, div, px, uniform_list,
};
use lines::Lines;
use look::Typeset as _;
use protocol::model::WorkspaceId;
use reading::Unshown;
use transcript::Document;

/// The height of a line of a file, and of a line of a diff, so the two read alike.
pub(crate) const LINE_HEIGHT: Pixels = look::CodeLines::File.height();

/// What the file view has to show for its file.
pub(super) enum Contents {
    Loading,
    Lines(Lines),
    Image(FileImage),
    Instead(Unshown),
}

pub struct FileView {
    pub(super) client: Client,
    pub(super) workspace_id: WorkspaceId,
    /// The file the file tab shows, while it shows one.
    pub path: Option<SharedString>,
    pub(super) contents: Contents,
    pub(super) scroll: UniformListScrollHandle,
    /// Where keys go while the file tab shows a file, so the window's keys reach the app.
    pub(super) focus: FocusHandle,
    /// How many times a file has been read, so only the newest answer is shown.
    pub(super) reads: u64,
    /// A hash of the bytes what's shown was made from, its lines or its image, so a file read again that hasn't changed
    /// is left as it is.
    pub(super) shown: Option<u64>,
    /// A Markdown file, rendered, which shows in place of its lines while the file tab's switch says so.
    pub(super) document: Entity<Document>,
    pub(super) _releasing: Subscription,
    _choosing: Subscription,
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
            document: cx.new(Document::new),
            _releasing: releasing,
            _choosing: cx.observe_global::<Chosen<MarkdownView>>(|_, cx| cx.notify()),
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

    /// What a copy of the whole of a Markdown file has, while it shows it rendered, or `None` while it doesn't.
    #[cfg(feature = "testing")]
    pub fn rendered(&self, cx: &App) -> Option<String> {
        self.shows_rendered(cx).then(|| self.document.read(cx).copied_whole())
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
    pub(super) fn set_contents(
        &mut self,
        read: u64,
        contents: Contents,
        shown: Option<u64>,
        cx: &mut Context<Self>,
    ) -> bool {
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

    pub(super) fn draw_lines(&self, range: std::ops::Range<usize>, cx: &App) -> Vec<AnyElement> {
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
                            .pr(look::Space::Large)
                            .text_right()
                            .text_color(colors.text_faint)
                            .child((index + 1).to_string()),
                    )
                    .child(
                        div().flex_none().pr(look::Space::Large).whitespace_nowrap().text_color(foreground).child(text),
                    );
                Some(row.into_any_element())
            })
            .collect()
    }

    /// Its path, and for an image, what kind it is and how big, at the other end.
    pub(super) fn header(&self, cx: &App) -> Option<AnyElement> {
        let path = self.path.as_ref()?;
        let about = match &self.contents {
            Contents::Image(image) => Some(
                div().flex_none().text_color(look::colors(cx).text_muted).child(image.describe()).into_any_element(),
            ),
            Contents::Lines(_) if is_markdown(path) => Some(switch::<MarkdownView>(cx)),
            _ => None,
        };
        Some(file_header(path, None, about, cx))
    }
}

impl FileView {
    /// Whether it shows a Markdown file rendered, rather than its lines.
    fn shows_rendered(&self, cx: &App) -> bool {
        self.path.as_deref().is_some_and(is_markdown) && chosen::<MarkdownView>(cx) == MarkdownView::Preview
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
            Contents::Loading => look::nothing_to_show("Loading…").into_any_element(),
            Contents::Image(image) => picture(image),
            Contents::Instead(why) => look::nothing_to_show(why.said()).into_any_element(),
            Contents::Lines(_) if self.shows_rendered(cx) => self.document.clone().into_any_element(),
            Contents::Lines(lines) => {
                uniform_list("file-lines", lines.lines.len(), cx.processor(|t, range, _, cx| t.draw_lines(range, cx)))
                    .track_scroll(&self.scroll)
                    .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
                    .with_width_from_item(Some(lines.widest))
                    .flex_1()
                    .w_full()
                    .py(look::Space::XSmall)
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
    div().flex_1().w_full().min_h_0().p(look::Space::XLarge).child(room.size_full()).into_any_element()
}
