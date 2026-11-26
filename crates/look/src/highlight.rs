//! Syntax highlighting for the diff, the transcript's tool calls and the code blocks of every text the kit draws, with
//! syntect and bat's syntax collection (two-face), in the colours of Visual Studio's dark theme.
//!
//! gpui-kit's `syntect` feature pulls these crates in, but its `highlight_code` is `pub(crate)`, so they're used
//! directly here. A large diff is highlighted on a background thread, which can wait for the syntaxes to load. A
//! tool call's few lines are highlighted where they're drawn, on the main thread, which can't, so they're drawn
//! plain until the syntaxes are ready.

use std::ops::Range;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui_kit::base::text::editor_host::CodeBlock;
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

/// The colours for a code block in a text the kit draws, such as a reply's, as [`highlight`] gives them, so code is in
/// the same colours there as in a diff. A block that names no language is left plain. The kit keeps each block's
/// colours, so a block drawn before [`preload_highlighting`] has finished would stay plain if this didn't wait for the
/// syntaxes. It waits on the main thread, as the kit's own highlighter loads them there, and only that once.
pub(crate) fn code_block(block: &CodeBlock) -> Spans {
    block.lang().map_or_default(|language| highlight(&language, &block.code()))
}

/// What to highlight a file as, by its path: its extension, or its whole name for one like `Makefile`.
pub fn language(path: &str) -> &str {
    let name = path.rsplit('/').find(|part| !part.is_empty()).unwrap_or(path);
    name.rsplit_once('.').map_or(name, |(_, extension)| extension)
}

/// The syntax for `language`, a file's extension or what a code block's fence names: as syntect knows it, by an
/// extension or by its own name, such as `tsx` or `javascript`, or else by the extension of what a fence often names in
/// its place.
fn find_syntax<'a>(syntaxes: &'a SyntaxSet, language: &str) -> Option<&'a SyntaxReference> {
    let instead = match language.to_lowercase().as_str() {
        "shell" | "zsh" => "sh",
        "csharp" | "c#" => "cs",
        "jsonc" => "json",
        "mdx" => "md",
        "ejs" => "erb",
        "make" => "Makefile",
        _ => language,
    };
    [language, instead]
        .into_iter()
        .find_map(|token| syntaxes.find_syntax_by_token(token))
        .filter(|syntax| syntax.name != "Plain Text")
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
    fn a_file_is_highlighted_by_its_extension_or_its_name() {
        assert_eq!(language("/workspaces/aspen/src/signal.rs"), "rs");
        assert_eq!(language("crates/look/src/theme.json"), "json");
        assert_eq!(language("/workspaces/aspen/Makefile"), "Makefile");
    }

    #[test]
    fn a_code_block_is_in_the_colours_a_diff_is_and_plain_with_no_language() {
        let code = "fn main() {}\n";

        assert_eq!(code_block(&CodeBlock::from_code(code, Some("rust"))), highlight("rust", code));
        assert!(!highlight("rust", code).is_empty());
        assert!(code_block(&CodeBlock::from_code(code, None::<&str>)).is_empty());
    }

    #[test]
    fn a_language_is_found_by_what_a_code_fence_calls_it() {
        let found = |language| find_syntax(syntaxes(), language).map(|syntax| syntax.name.as_str());

        assert_eq!(found("tsx"), Some("TypeScriptReact"), "by its own extension, not as plain TypeScript");
        assert_eq!(found("Python"), Some("Python"), "by its name, whatever its case");
        for language in ["csharp", "c#", "jsonc", "mdx", "ejs", "make", "shell", "zsh"] {
            assert!(found(language).is_some(), "{language}");
        }
    }

    #[test]
    fn a_language_it_doesn_t_know_is_left_plain() {
        assert!(highlight("no-such-language", "anything").is_empty());
    }
}
