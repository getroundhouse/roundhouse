//! A file's text as the lines the file view draws: each tab drawn as spaces, and each line with its own colours, cut
//! from the colours of the whole file, since a comment or a string can run across lines.

use std::ops::Range;

use gpui_kit::{HighlightStyle, SharedString};

/// What a tab is drawn as, as the transcript draws a file's lines.
pub(crate) const TAB: &str = "    ";

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Line {
    pub(crate) text: SharedString,
    /// Its colours, by byte range within it, in order and not overlapping.
    pub(crate) styles: Vec<(Range<usize>, HighlightStyle)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Lines {
    pub(crate) lines: Vec<Line>,
    /// Which line is widest, which sets how far the view scrolls sideways.
    pub(crate) widest: usize,
}

impl Lines {
    /// `text`'s lines, coloured as `language` if it's given, which waits for the highlighter, so it's for a background
    /// thread. A line ending at the very end doesn't start another line.
    pub(crate) fn new(text: &str, language: Option<&str>) -> Self {
        let text = text.replace('\t', TAB);
        let spans = language.map_or_default(|language| look::highlight(language, &text));
        let mut ranges = Vec::new();
        let mut start = 0;
        for line in text.split_inclusive('\n') {
            let end = start + line.trim_end_matches(['\n', '\r']).len();
            ranges.push(start..end);
            start += line.len();
        }
        let widest = ranges.iter().enumerate().max_by_key(|(_, range)| text[(*range).clone()].chars().count());
        let widest = widest.map_or(0, |(index, _)| index);
        let mut next = 0;
        let lines = ranges
            .into_iter()
            .map(|range| {
                // Spans are in order, so each line starts looking where the last one's left off, apart from one that
                // runs on from the line before.
                while spans.get(next).is_some_and(|(span, _)| span.end <= range.start) {
                    next += 1;
                }
                let styles = spans[next.min(spans.len())..]
                    .iter()
                    .take_while(|(span, _)| span.start < range.end)
                    .map(|(span, style)| {
                        (span.start.max(range.start) - range.start..span.end.min(range.end) - range.start, *style)
                    })
                    .filter(|(span, _)| !span.is_empty())
                    .collect();
                Line { text: text[range].to_string().into(), styles }
            })
            .collect();
        Self { lines, widest }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(lines: &Lines) -> Vec<&str> {
        lines.lines.iter().map(|line| line.text.as_ref()).collect()
    }

    #[test]
    fn a_file_splits_into_its_lines_without_their_endings() {
        let lines = Lines::new("one\r\ntwo\n\nthree, the widest\n", None);

        assert_eq!(texts(&lines), ["one", "two", "", "three, the widest"]);
        assert_eq!(lines.widest, 3);
        assert_eq!(texts(&Lines::new("no ending", None)), ["no ending"]);
        assert!(Lines::new("", None).lines.is_empty());
    }

    #[test]
    fn a_tab_is_drawn_as_spaces() {
        assert_eq!(texts(&Lines::new("\tlet x = 1;\n", None)), ["    let x = 1;"]);
    }

    #[test]
    fn a_colour_that_runs_across_lines_is_cut_at_each() {
        look::preload_highlighting();

        let lines = Lines::new("/* one\ntwo */ fn main() {}\n", Some("rs"));

        let first = &lines.lines[0];
        assert_eq!(first.styles.first().map(|(range, _)| range.clone()), Some(0..6), "the whole first line");
        let second = &lines.lines[1];
        let comment = second.styles.first().map(|(range, style)| (range.clone(), *style));
        assert_eq!(comment, Some((0..6, first.styles[0].1)), "to the comment's end, in the comment's colour");
        assert!(second.styles.iter().all(|(range, _)| range.end <= second.text.len()), "{:?}", second.styles);
    }
}
