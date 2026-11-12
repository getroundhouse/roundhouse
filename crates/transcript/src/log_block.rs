//! A tool's output of any size. Its height comes from its line count, and only the lines on screen get
//! shaped, so a 2 MB result costs a line table and about 50 lines of text. It draws the matches of what's being found
//! on the lines it shapes.

use std::borrow::Cow;
use std::sync::Arc;

use gpui_kit::{
    App, Bounds, Element, ElementId, GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId, Pixels,
    SharedString, Size, Style, TextAlign, TextRun, Window, font, point, px, relative, size,
};

use crate::find::Marks;

pub(crate) const LINE_HEIGHT: f32 = 17.0;
/// Keeps one very long line from costing more than the screen can show.
const MAX_LINE_BYTES: usize = 2000;
/// What a tab is drawn as in monospace, here and in a file's lines.
pub(crate) const TAB: &str = "    ";

#[derive(Clone)]
pub(crate) struct LogText {
    pub text: SharedString,
    /// The byte offset each line starts at.
    pub lines: Arc<Vec<u32>>,
}

#[expect(clippy::cast_precision_loss, reason = "a position on screen, which only drifts past 16 million lines")]
fn line_top(line: usize) -> Pixels {
    px(line as f32 * LINE_HEIGHT)
}

/// The line at a position measured in lines.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "dropping the fraction is the point, and `as` turns anything negative into 0"
)]
fn line_at(lines: f32) -> usize {
    lines as usize
}

/// What's drawn of `text`: each line cut short at [`MAX_LINE_BYTES`], on a character boundary. Borrowed when no line
/// is that long, which is almost always.
pub(crate) fn shown(text: &str) -> Cow<'_, str> {
    if text.split('\n').all(|line| line.len() <= MAX_LINE_BYTES) {
        return Cow::Borrowed(text);
    }
    Cow::Owned(text.split('\n').map(cut).collect::<Vec<_>>().join("\n"))
}

/// What's drawn of a line: as much as [`MAX_LINE_BYTES`] holds, on a character boundary.
fn cut(line: &str) -> &str {
    let mut end = line.len().min(MAX_LINE_BYTES);
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    &line[..end]
}

/// Where `offset` in `text` is once each tab before it is drawn as [`TAB`].
pub(crate) fn tabbed(text: &str, offset: usize) -> usize {
    let tabs = text.char_indices().take_while(|(at, _)| *at < offset).filter(|(_, c)| *c == '\t').count();
    offset + tabs * (TAB.len() - 1)
}

impl LogText {
    /// `text`, as [`shown`] has it.
    pub(crate) fn new(text: &str) -> Self {
        let mut lines = vec![0u32];
        lines.extend(
            text.bytes()
                .enumerate()
                .filter(|(_, b)| *b == b'\n')
                .map(|(i, _)| u32::try_from(i + 1).unwrap_or(u32::MAX)),
        );
        Self { text: SharedString::from(text.to_string()), lines: Arc::new(lines) }
    }

    /// Where line `index` starts and ends in the text.
    fn span(&self, index: usize) -> (usize, usize) {
        let start = self.lines[index] as usize;
        let end = self.lines.get(index + 1).map_or(self.text.len(), |next| *next as usize - 1);
        (start, start + cut(&self.text[start..end]).len())
    }

    #[cfg(test)]
    fn line(&self, index: usize) -> &str {
        let (start, end) = self.span(index);
        &self.text[start..end]
    }
}

pub(crate) struct LogBlock {
    pub log: LogText,
    pub color: Hsla,
    pub font: SharedString,
    /// Where the matches are in the text.
    pub marks: Marks,
    /// The area it's in that scrolls, which the match being shown says it's in.
    pub area: Option<ElementId>,
}

impl IntoElement for LogBlock {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for LogBlock {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let height = line_top(self.log.lines.len());
        let style = Style { size: Size { width: relative(1.).into(), height: height.into() }, ..Default::default() };
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        (): &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut (),
        (): &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.say_where_the_match_is(bounds);
        let clip = window.content_mask().bounds;
        let top = (clip.top() - bounds.top()).max(px(0.));
        let bottom = (clip.bottom() - bounds.top()).max(px(0.));
        let first = line_at(f32::from(top) / LINE_HEIGHT);
        let last = line_at((f32::from(bottom) / LINE_HEIGHT).ceil()).min(self.log.lines.len());
        let font = font(self.font.clone());
        for line in first..last {
            let (start, end) = self.log.span(line);
            let raw = &self.log.text[start..end];
            let text: SharedString = raw.replace('\t', TAB).into();
            if text.is_empty() {
                continue;
            }
            let run = TextRun {
                len: text.len(),
                font: font.clone(),
                color: self.color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let shaped = window.text_system().shape_line(text, px(12.), &[run], None);
            let origin = point(bounds.left(), bounds.top() + line_top(line));
            for (range, color) in self.marks.within(start, end).backgrounds() {
                let (from, to) =
                    (shaped.x_for_index(tabbed(raw, range.start)), shaped.x_for_index(tabbed(raw, range.end)));
                let quad = Bounds::new(point(origin.x + from, origin.y), size(to - from, px(LINE_HEIGHT)));
                window.paint_quad(gpui_kit::fill(quad, color));
            }
            let _ = shaped.paint(origin, px(LINE_HEIGHT), TextAlign::Left, None, window, cx);
        }
    }
}

impl LogBlock {
    /// Says where the match being shown is, if it's in this, by its line. Its line may be far from what's on screen,
    /// and isn't shaped, so it says where the line starts.
    fn say_where_the_match_is(&self, bounds: Bounds<Pixels>) {
        let (Some(here), Some(reveal)) = (self.marks.here(), self.marks.reveal()) else { return };
        let line = self.log.lines.partition_point(|start| *start as usize <= here.start).saturating_sub(1);
        let at = Bounds::new(point(bounds.left(), bounds.top() + line_top(line)), size(px(1.), px(LINE_HEIGHT)));
        reveal.drawn_at(at, self.area.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::{LogText, MAX_LINE_BYTES, shown, tabbed};

    #[test]
    fn lines_are_found_without_copying_the_text() {
        let log = LogText::new("one\ntwo\n\nfour");
        assert_eq!(log.lines.len(), 4);
        assert_eq!(log.line(0), "one");
        assert_eq!(log.line(1), "two");
        assert_eq!(log.line(2), "");
        assert_eq!(log.line(3), "four");
    }

    #[test]
    fn a_very_long_line_is_cut_on_a_character_boundary() {
        let text = "é".repeat(MAX_LINE_BYTES);
        let log = LogText::new(&text);
        let line = log.line(0);
        assert!(line.len() <= MAX_LINE_BYTES);
        assert!(line.ends_with('é'));
        assert_eq!(shown(&format!("{text}\nnext")), format!("{line}\nnext"), "what's shown is what's drawn");
    }

    #[test]
    fn a_trailing_newline_is_an_empty_last_line() {
        let log = LogText::new("one\n");
        assert_eq!(log.lines.len(), 2);
        assert_eq!(log.line(1), "");
    }

    #[test]
    fn a_tab_pushes_what_follows_it_along() {
        assert_eq!(tabbed("\ta\tb", 1), 4);
        assert_eq!(tabbed("\ta\tb", 3), 9);
        assert_eq!(tabbed("ab", 2), 2);
    }
}
