//! Syntax highlighting for the diff, the transcript's tool calls and the code blocks of every text the kit draws, with
//! syntect and bat's syntax collection (two-face), in Visual Studio's Dark+ with each of its colours taken from the role
//! that stands for it in `look::Colors`.
//!
//! gpui-kit's `syntect` feature pulls these crates in, but its `highlight_code` is `pub(crate)`, so they're used
//! directly here. A large diff is highlighted on a background thread, which can wait for the syntaxes to load. A
//! tool call's few lines are highlighted where they're drawn, on the main thread, which can't, so they're drawn
//! plain until the syntaxes are ready.

use std::ops::Range;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui_kit::base::text::editor_host::CodeBlock;
use gpui_kit::{HighlightStyle, Hsla, Rgba};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Color, Theme};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

use crate::Colors;

static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
static THEME: OnceLock<Theme> = OnceLock::new();
/// Whether [`preload_highlighting`] has finished, so highlighting won't wait on anything.
static READY: AtomicBool = AtomicBool::new(false);

/// Each span of `code` that has a colour, by its byte range.
pub type Spans = Vec<(Range<usize>, HighlightStyle)>;

fn syntaxes() -> &'static SyntaxSet {
    SYNTAXES.get_or_init(two_face::syntax::extra_newlines)
}

fn theme() -> &'static Theme {
    THEME.get_or_init(|| in_roles(dark_plus(), &Colors::dark()))
}

#[expect(deprecated, reason = "two-face will rename this variant in 0.5")]
fn dark_plus() -> Theme {
    two_face::theme::extra().get(two_face::theme::EmbeddedThemeName::VisualStudioDarkPlus).clone()
}

/// `theme` with each colour it draws code in swapped for the role that stands for it. Its colours for a git gutter,
/// which no syntax gives, stand for nothing, so what draws with them goes.
fn in_roles(mut theme: Theme, colors: &Colors) -> Theme {
    theme.settings.foreground = Some(syntect_color(colors.text));
    theme.scopes = (theme.scopes.into_iter())
        .filter_map(|mut item| {
            if let Some(drawn) = item.style.foreground {
                item.style.foreground = Some(syntect_color(role(drawn, colors)?));
            }
            Some(item)
        })
        .collect();
    theme
}

/// The role that stands for a colour Dark+ draws code in, by Dark+'s value for it. Its plain colour, and what it draws
/// as plain, such as an operator, is `text`.
fn role(dark_plus: Color, c: &Colors) -> Option<Hsla> {
    Some(match u32::from_be_bytes([0, dark_plus.r, dark_plus.g, dark_plus.b]) {
        0xdc_dcdc => c.text,
        0xc5_86c0 => c.code_keyword,
        0x56_9cd6 => c.code_storage,
        0x60_8b4e => c.code_comment,
        0xd6_9d85 => c.code_string,
        0xe3_bbab => c.code_escape,
        0xb5_cea8 => c.code_number,
        0xb4_cea8 => c.code_constant,
        0xdc_dcaa => c.code_function,
        0x4e_c9b0 => c.code_type,
        0x9c_dcfe => c.code_variable,
        0x92_caf4 => c.code_attribute,
        0x80_8080 => c.code_tag,
        0xc8_c8c8 => c.code_value,
        0xd7_ba7d => c.code_selector,
        0x87_cefa => c.code_at_rule,
        0x9b_9b9b => c.code_preprocessor,
        0x68_685b => c.code_processing,
        0x68_7687 => c.code_js_operator,
        0x70_727e => c.code_method_storage,
        _ => return None,
    })
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a channel is between 0 and 1, so it rounds to a byte"
)]
fn syntect_color(color: Hsla) -> Color {
    let Rgba { r, g, b, a } = color.into();
    let [r, g, b, a] = [r, g, b, a].map(|channel| (channel.clamp(0., 1.) * 255.).round() as u8);
    Color { r, g, b, a }
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

fn color(c: Color) -> HighlightStyle {
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
    use syntect::highlighting::ThemeItem;

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

    /// Whether `item` is one of Dark+'s colours for a git gutter, which no syntax gives.
    fn a_gutters(item: &ThemeItem) -> bool {
        let scopes = item.scope.selectors.iter().flat_map(|selector| selector.path.as_slice());
        scopes.map(|scope| scope.build_string()).any(|scope| scope.ends_with(".git_gutter"))
    }

    #[test]
    fn dark_pluss_colours_and_the_roles_for_code_are_one_set() {
        let colors = Colors::dark();
        let drawn = dark_plus().scopes.into_iter().filter(|item| !a_gutters(item));
        let of_dark_plus: Vec<Color> = drawn.filter_map(|item| item.style.foreground).collect();

        let unnamed: Vec<_> = of_dark_plus.iter().filter(|colour| role(**colour, &colors).is_none()).collect();
        assert!(unnamed.is_empty(), "colours of Dark+'s that no role stands for: {unnamed:?}");
        let named: Vec<Hsla> = of_dark_plus.iter().filter_map(|colour| role(*colour, &colors)).collect();
        let code = colors.roles().into_iter().filter(|role| role.group == "Code");
        let unused: Vec<_> = code.filter(|role| !named.contains(&role.color)).map(|role| role.name).collect();
        assert!(unused.is_empty(), "roles for code that stand for none of Dark+'s colours: {unused:?}");
    }

    #[test]
    fn code_is_in_dark_pluss_colours_but_plain_code_is_in_text() {
        let text = syntect_color(Colors::dark().text);
        let plain = Color { r: 0xdc, g: 0xdc, b: 0xdc, a: 0xff };
        let dark_plus = dark_plus();
        let kept: Vec<_> = dark_plus.scopes.iter().filter(|item| !a_gutters(item)).collect();

        assert_eq!(dark_plus.settings.foreground, Some(plain));
        assert_eq!(theme().settings.foreground, Some(text));
        assert_eq!(kept.len(), theme().scopes.len());
        for (theirs, ours) in kept.into_iter().zip(&theme().scopes) {
            let expected = theirs.style.foreground.map(|colour| if colour == plain { text } else { colour });
            assert_eq!((&ours.scope, ours.style.foreground), (&theirs.scope, expected));
        }
    }

    #[test]
    fn a_language_it_doesn_t_know_is_left_plain() {
        assert!(highlight("no-such-language", "anything").is_empty());
    }
}
