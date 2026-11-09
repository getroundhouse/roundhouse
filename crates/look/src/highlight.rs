//! Syntax highlighting for the diff and the transcript's tool calls, with syntect and bat's syntax collection
//! (two-face), in the colours of Visual Studio's dark theme.
//!
//! gpui-kit's `syntect` feature pulls these crates in, but its `highlight_code` is `pub(crate)`, so they're used
//! directly here. A large diff is highlighted on a background thread, which can wait for the syntaxes to load. A
//! tool call's few lines are highlighted where they're drawn, on the main thread, which can't, so they're drawn
//! plain until the syntaxes are ready.

use std::ops::Range;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui_kit::{HighlightStyle, Rgba};
use syntect::easy::HighlightLines;
use syntect::highlighting::Theme;
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
static THEME: OnceLock<Theme> = OnceLock::new();
/// Whether [`preload_highlighting`] has finished, so highlighting won't wait on anything.
static READY: AtomicBool = AtomicBool::new(false);

/// Each span of `code` that has a colour, by its byte range.
pub type Spans = Vec<(Range<usize>, HighlightStyle)>;

fn syntaxes() -> &'static SyntaxSet {
    SYNTAXES.get_or_init(two_face::syntax::extra_newlines)
}

#[expect(deprecated, reason = "two-face will rename this variant in 0.5")]
fn theme() -> &'static Theme {
    THEME.get_or_init(|| two_face::theme::extra().get(two_face::theme::EmbeddedThemeName::VisualStudioDarkPlus).clone())
}

/// Loads the syntaxes and the theme, and the languages most often shown. Deserializing the syntax set takes tens of
/// milliseconds, so the app does this off the main thread at launch.
pub fn preload_highlighting() {
    let samples = [
        ("rust", "fn main() {}\n"),
        ("ts", "const a = 1;\n"),
        ("python", "a = 1\n"),
        ("bash", "ls -la\n"),
        ("json", "{\"a\": 1}\n"),
    ];
    for (language, code) in samples {
        let _ = highlight(language, code);
    }
    READY.store(true, Ordering::Release);
}

/// The colours for `code` as `language`. Empty when the language is unknown. Waits for the syntaxes to load, so
/// it's for a background thread.
pub fn highlight(language: &str, code: &str) -> Spans {
    let Some(syntax) = find_syntax(syntaxes(), language) else {
        return Vec::new();
    };
    let mut lines = HighlightLines::new(syntax, theme());
    let mut out = Vec::new();
    let mut offset = 0usize;
    for line in LinesWithEndings::from(code) {
        let Ok(regions) = lines.highlight_line(line, syntaxes()) else {
            break;
        };
        for (style, text) in regions {
            push_span(&mut out, offset..offset + text.len(), color(style.foreground));
            offset += text.len();
        }
    }
    out
}

/// The colours for `code`, as [`highlight`] gives them, or nothing until [`preload_highlighting`] has finished, so
/// the main thread never waits for it.
pub fn try_highlight(language: &str, code: &str) -> Option<Spans> {
    READY.load(Ordering::Acquire).then(|| highlight(language, code))
}

fn find_syntax<'a>(syntaxes: &'a SyntaxSet, language: &str) -> Option<&'a SyntaxReference> {
    let token = match language {
        "ts" | "tsx" | "typescript" => "ts",
        "js" | "javascript" | "jsx" => "js",
        "rs" | "rust" => "rs",
        "py" | "python" => "py",
        "sh" | "shell" | "bash" | "zsh" => "sh",
        other => other,
    };
    syntaxes.find_syntax_by_token(token).filter(|s| s.name != "Plain Text")
}

fn color(c: syntect::highlighting::Color) -> HighlightStyle {
    let rgba = Rgba { r: f32::from(c.r) / 255.0, g: f32::from(c.g) / 255.0, b: f32::from(c.b) / 255.0, a: 1.0 };
    HighlightStyle { color: Some(rgba.into()), ..HighlightStyle::default() }
}

/// Adds a span, joined to the one before when they touch and share a colour.
fn push_span(out: &mut Spans, range: Range<usize>, style: HighlightStyle) {
    if range.is_empty() {
        return;
    }
    if let Some((last, last_style)) = out.last_mut()
        && last.end == range.start
        && *last_style == style
    {
        last.end = range.end;
        return;
    }
    out.push((range, style));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_is_highlighted_only_once_the_syntaxes_are_ready() {
        assert!(try_highlight("rust", "fn main() {}").is_none(), "nothing has loaded them");
        preload_highlighting();

        let spans = try_highlight("rust", "fn main() {}").unwrap();

        assert_eq!(spans.first().map(|(range, _)| range.clone()), Some(0..2), "`fn` has a colour of its own");
        assert!(spans.windows(2).all(|pair| pair[0].0.end <= pair[1].0.start), "in order, without overlapping");
    }

    #[test]
    fn a_language_it_doesn_t_know_is_left_plain() {
        assert!(highlight("no-such-language", "anything").is_empty());
    }
}
