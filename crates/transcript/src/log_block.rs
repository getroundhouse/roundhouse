//! A tool's output of any size. Its height comes from its line count, and only the lines on screen get
//! shaped, so a 2 MB result costs a line table and about 50 lines of text.

use std::sync::Arc;

use gpui_kit::*;

pub const LINE_HEIGHT: f32 = 17.0;
/// Keeps one very long line from costing more than the screen can show.
const MAX_LINE_BYTES: usize = 2000;

#[derive(Clone)]
pub struct LogText {
    pub text: SharedString,
    /// The byte offset each line starts at.
    pub lines: Arc<Vec<u32>>,
}

impl LogText {
    pub fn new(text: &str) -> Self {
        let mut lines = vec![0u32];
        lines.extend(text.bytes().enumerate().filter(|(_, b)| *b == b'\n').map(|(i, _)| i as u32 + 1));
        Self { text: SharedString::from(text.to_string()), lines: Arc::new(lines) }
    }

    fn line(&self, index: usize) -> &str {
        let start = self.lines[index] as usize;
        let end = self.lines.get(index + 1).map(|next| *next as usize - 1).unwrap_or(self.text.len());
        let mut end = end.min(start + MAX_LINE_BYTES);
        while !self.text.is_char_boundary(end) {
            end -= 1;
        }
        &self.text[start..end]
    }
}

pub struct LogBlock {
    pub log: LogText,
    pub color: Hsla,
    pub font: SharedString,
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
        let height = px(self.log.lines.len() as f32 * LINE_HEIGHT);
        let style = Style { size: Size { width: relative(1.).into(), height: height.into() }, ..Default::default() };
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let clip = window.content_mask().bounds;
        let top = (clip.top() - bounds.top()).max(px(0.));
        let bottom = (clip.bottom() - bounds.top()).max(px(0.));
        let first = (f32::from(top) / LINE_HEIGHT) as usize;
        let last = ((f32::from(bottom) / LINE_HEIGHT).ceil() as usize).min(self.log.lines.len());
        let font = font(self.font.clone());
        for line in first..last {
            let text: SharedString = self.log.line(line).replace('\t', "    ").into();
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
            let origin = point(bounds.left(), bounds.top() + px(line as f32 * LINE_HEIGHT));
            let _ = shaped.paint(origin, px(LINE_HEIGHT), TextAlign::Left, None, window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LogText, MAX_LINE_BYTES};

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
        let long = "é".repeat(MAX_LINE_BYTES);
        let log = LogText::new(&long);
        let line = log.line(0);
        assert!(line.len() <= MAX_LINE_BYTES);
        assert!(line.ends_with('é'));
    }

    #[test]
    fn a_trailing_newline_is_an_empty_last_line() {
        let log = LogText::new("one\n");
        assert_eq!(log.lines.len(), 2);
        assert_eq!(log.line(1), "");
    }
}
