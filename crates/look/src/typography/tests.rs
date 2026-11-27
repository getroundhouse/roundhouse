use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{AbsoluteLength, App, Div, Styled as _, TestAppContext, div};

use super::styles::EMPHASIS;
use super::{CodeLines, Type, Typeset as _};
use crate::spec_table;
use crate::traits::Reach;

#[test]
fn the_specs_table_has_every_kind_of_text_in_order_as_the_app_sets_it() {
    let table: Vec<[String; 5]> = spec_table::rows("**Every kind of text, by its name.**");

    let set: Vec<[String; 5]> = Type::ALL.iter().map(|kind| kind.written()).collect();
    assert_eq!(table, set, "docs/spec/look.md's table of the kinds of text, against crates/look/src/typography/mod.rs");
}

/// `kind`, set as what's set in it is.
fn set_as(kind: Type, cx: &App) -> Div {
    match kind {
        Type::Code => div().typeset_code(cx),
        kind @ (Type::PageTitle | Type::Heading | Type::Body | Type::Detail | Type::Key | Type::KeyHeld) => {
            div().typeset(kind)
        }
    }
}

#[gpui_kit::test]
fn each_kind_sets_its_size_its_weight_and_its_lines(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    cx.update(|cx| {
        for &kind in Type::ALL {
            let mut set = set_as(kind, cx);
            let text = &set.style().text;

            assert_eq!(text.font_size, Some(kind.set().size), "{kind:?}");
            assert_eq!(text.font_weight, kind.set().weight, "{kind:?}");
            assert_eq!(text.line_height, kind.set().line, "{kind:?}");
        }
    });
}

/// What's held with a key is smaller than the key, and sits on its line.
#[test]
fn only_a_keys_kinds_have_lines_of_their_own_as_tall_as_what_the_key_says() {
    for reach in [Reach::Desk, Reach::Handheld] {
        let key = Type::Key.set_at(reach).size;

        for &kind in Type::ALL {
            let on_a_key = matches!(kind, Type::Key | Type::KeyHeld);
            assert_eq!(kind.set_at(reach).line, on_a_key.then(|| key.into()), "{kind:?} at {reach:?}");
        }
    }
}

/// So none drifts from the rest, or from the kit's own text, if the rem changes.
#[test]
fn every_kind_is_in_rems_at_a_desk() {
    for &kind in Type::ALL {
        assert!(matches!(kind.set_at(Reach::Desk).size, AbsoluteLength::Rems(_)), "{kind:?}");
    }
}

#[gpui_kit::test]
fn emphasis_is_medium_over_any_kind(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    cx.update(|cx| {
        for &kind in Type::ALL {
            let mut set = set_as(kind, cx).emphasis();

            assert_eq!(set.style().text.font_weight, Some(EMPHASIS), "{kind:?}");
        }
    });
}

/// On a Linux desktop without the font GPUI gives `.SystemUIFont`, text that named it would fall back to a family in
/// its regular face whatever weight it asked for, where the window's family, which the kit's theme names, keeps its
/// medium face. Code names the family the kit's theme names, which the kit picks from those installed, since one named
/// here wouldn't be on every platform.
#[gpui_kit::test]
fn only_code_names_a_font_the_one_the_kits_theme_names(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(crate::apply);

    cx.update(|cx| {
        for &kind in Type::ALL {
            let mut set = set_as(kind, cx);
            let named = set.style().text.font_family.clone();

            let code = (kind == Type::Code).then(|| cx.theme().mono_font_family.clone());
            assert_eq!(named, code, "{kind:?}");
        }
    });
}

/// Code set as the system font's kinds are would be in the window's font, as nothing would show until it was drawn.
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "typeset_code")]
fn code_is_only_set_with_typeset_code() {
    let _ = div().typeset(Type::Code);
}

/// The spec's sizes for Markdown's headings and inline code are `look`'s shares of body text's size at each reach.
#[test]
fn the_specs_table_has_every_share_of_markdown_at_each_reach() {
    let table: Vec<[String; 3]> =
        spec_table::rows("**Markdown's headings and inline code, as shares of the text they're in,**");

    let at = |share: f32, reach| {
        let size = f32::from(Type::Body.set_at(reach).size.to_pixels(super::styles::rem(reach)) * share);
        format!("{}px", (size * 100.).round() / 100.)
    };
    let headings = (1..).zip(super::MARKDOWN_HEADINGS).map(|(level, share)| (format!("Heading {level}"), share));
    let shares: Vec<[String; 3]> = headings
        .chain([("Inline code".to_string(), super::INLINE_CODE)])
        .map(|(name, share)| [name, at(share, Reach::Desk), at(share, Reach::Handheld)])
        .collect();
    assert_eq!(table, shares, "docs/spec/look.md's table of Markdown's sizes, against crates/look/src/typography");
}

#[test]
fn the_specs_table_has_every_spacing_of_codes_lines_in_order() {
    let table: Vec<[String; 3]> = spec_table::rows("**How far apart code's lines are, by where it's read.**");

    let spaced: Vec<[String; 3]> = CodeLines::ALL.iter().map(|lines| lines.written()).collect();
    assert_eq!(table, spaced, "docs/spec/look.md's table of code's line heights, against crates/look/src/typography");
}
