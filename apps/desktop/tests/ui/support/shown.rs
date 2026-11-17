//! What the window shows, in words, for a wait that times out to say what it found instead.

#![cfg(test)]

use gpui_kit::AppContext as _;
use gpui_kit::base::test_support::{TextSnapshot, snapshots, texts};
use gpui_kit::test::ElementSnapshot;

use super::Ui;

/// The most of each text that's said, in characters, so a long line, such as an agent's reply, doesn't hide the rest.
const EACH_AT_MOST: usize = 80;
/// The most that's said in all, in bytes, so a failure's message stays a few lines of a CI log.
const ALL_AT_MOST: usize = 4096;
/// What goes between two texts.
const BETWEEN: &str = " | ";

impl Ui<'_> {
    /// What the window shows: the texts the kit painted, which are its labels' and text views', such as the chat's,
    /// and then, from the top down, the labels the window gives assistive apps of what shows, with what each holds,
    /// such as an input, which say much of what the app draws as plain text, since the kit notes none of that. Each
    /// is on one line, cut short past [`EACH_AT_MOST`], and all of it past [`ALL_AT_MOST`]. Only what shows, so not
    /// what's scrolled out of view. It draws the whole window first, since a frame paints only the views that
    /// changed, and the kit notes only what a frame paints.
    pub(crate) fn texts_shown(&mut self) -> String {
        let shown = self.cx.update_window(self.handle, |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
            let painted = texts(window).into_iter().filter(TextSnapshot::visible).map(|text| text.text().to_string());
            let mut elements: Vec<_> = snapshots(window).into_iter().filter(ElementSnapshot::visible).collect();
            elements.sort_by(|a, b| {
                let (a, b) = (a.bounds().origin, b.bounds().origin);
                a.y.cmp(&b.y).then(a.x.cmp(&b.x))
            });
            let labelled = elements.iter().filter_map(|element| {
                let label = element.label()?;
                Some(match element.value().filter(|value| !value.is_empty()) {
                    Some(value) => format!("{label}: {value}"),
                    None => label.to_string(),
                })
            });
            painted.chain(labelled).collect::<Vec<_>>()
        });
        let shown = joined(shown.unwrap_or_default());
        if shown.is_empty() { "the window shows nothing".to_string() } else { format!("the window shows {shown}") }
    }
}

/// `texts`, each on a line of its own cut short, between [`BETWEEN`]s, and cut short in all.
fn joined(texts: Vec<String>) -> String {
    let mut joined = String::new();
    for text in texts {
        let line = cut_short(&text.split_whitespace().collect::<Vec<_>>().join(" "));
        if line.is_empty() {
            continue;
        }
        if joined.len() + BETWEEN.len() + line.len() > ALL_AT_MOST {
            joined.push_str(BETWEEN);
            joined.push('…');
            break;
        }
        if !joined.is_empty() {
            joined.push_str(BETWEEN);
        }
        joined.push_str(&line);
    }
    joined
}

/// `line`, or its first [`EACH_AT_MOST`] characters and an ellipsis.
fn cut_short(line: &str) -> String {
    match line.char_indices().nth(EACH_AT_MOST) {
        Some((at, _)) => format!("{}…", &line[..at]),
        None => line.to_string(),
    }
}
