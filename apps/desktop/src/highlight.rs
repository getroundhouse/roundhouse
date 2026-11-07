//! Syntax highlighting for diffs, with syntect and bat's syntax collection (two-face).
//!
//! gpui-kit's `syntect` feature pulls these crates in, but its `highlight_code` is `pub(crate)`, so
//! they're used directly here. Only runs on background threads.

use std::ops::Range;
use std::sync::LazyLock;

use gpui_kit::{Hsla, Rgba};
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Theme};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

pub(crate) static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(two_face::syntax::extra_newlines);
#[expect(deprecated, reason = "two-face will rename this variant in 0.5")]
pub(crate) static THEME: LazyLock<Theme> =
    LazyLock::new(|| two_face::theme::extra().get(two_face::theme::EmbeddedThemeName::VisualStudioDarkPlus).clone());

/// Deserializing the syntax set takes tens of milliseconds, so do it off the main thread at launch.
pub(crate) fn preload() {
    LazyLock::force(&SYNTAXES);
    LazyLock::force(&THEME);
    let _ = highlight("rust", "fn main() {}\n");
    let _ = highlight("ts", "const a = 1;\n");
    let _ = highlight("python", "a = 1\n");
    let _ = highlight("bash", "ls -la\n");
    let _ = highlight("json", "{\"a\": 1}\n");
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct HlStyle {
    pub color: Hsla,
    pub bold: bool,
    pub italic: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct HlSpan {
    pub range: Range<usize>,
    pub style: HlStyle,
}

pub(crate) fn find_syntax(lang: &str) -> Option<&'static SyntaxReference> {
    let token = match lang {
        "ts" | "tsx" | "typescript" => "ts",
        "js" | "javascript" | "jsx" => "js",
        "rs" | "rust" => "rs",
        "py" | "python" => "py",
        "sh" | "shell" | "bash" | "zsh" => "sh",
        other => other,
    };
    SYNTAXES.find_syntax_by_token(token).filter(|s| s.name != "Plain Text")
}

fn to_style(style: syntect::highlighting::Style) -> HlStyle {
    let c = style.foreground;
    let rgba = Rgba { r: f32::from(c.r) / 255.0, g: f32::from(c.g) / 255.0, b: f32::from(c.b) / 255.0, a: 1.0 };
    HlStyle {
        color: rgba.into(),
        bold: style.font_style.contains(FontStyle::BOLD),
        italic: style.font_style.contains(FontStyle::ITALIC),
    }
}

/// Spans for a whole code block, byte ranges into `code`. Empty when the language is unknown.
pub(crate) fn highlight(lang: &str, code: &str) -> Vec<HlSpan> {
    let Some(syntax) = find_syntax(lang) else {
        return Vec::new();
    };
    let mut hl = HighlightLines::new(syntax, &THEME);
    let mut out = Vec::new();
    let mut offset = 0usize;
    for line in LinesWithEndings::from(code) {
        let Ok(regions) = hl.highlight_line(line, &SYNTAXES) else {
            break;
        };
        for (style, text) in regions {
            push_span(&mut out, offset, text.len(), to_style(style));
            offset += text.len();
        }
    }
    out
}

fn push_span(out: &mut Vec<HlSpan>, offset: usize, len: usize, style: HlStyle) {
    if len == 0 {
        return;
    }
    if let Some(last) = out.last_mut()
        && last.range.end == offset
        && last.style.color == style.color
        && last.style.bold == style.bold
        && last.style.italic == style.italic
    {
        last.range.end = offset + len;
        return;
    }
    out.push(HlSpan { range: offset..offset + len, style });
}
