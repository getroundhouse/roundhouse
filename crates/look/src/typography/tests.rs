use gpui_kit::{AbsoluteLength, SharedString, Styled as _, div};

use super::styles::EMPHASIS;
use super::{CodeLines, MONO, Type, Typeset as _};
use crate::spec_table;
use crate::traits::Reach;

#[test]
fn the_specs_table_has_every_kind_of_text_in_order_as_the_app_sets_it() {
    let table: Vec<[String; 5]> = spec_table::rows("**Every kind of text, by its name.**");

    let set: Vec<[String; 5]> = Type::ALL.iter().map(|kind| kind.written()).collect();
    assert_eq!(table, set, "docs/spec/look.md's table of the kinds of text, against crates/look/src/typography/mod.rs");
}

#[test]
fn each_kind_sets_its_size_its_weight_and_its_lines() {
    for &kind in Type::ALL {
        let mut set = div().typeset(kind);
        let text = &set.style().text;

        assert_eq!(text.font_size, Some(kind.set().size), "{kind:?}");
        assert_eq!(text.font_weight, kind.set().weight, "{kind:?}");
        assert_eq!(text.line_height, kind.set().line, "{kind:?}");
    }
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

#[test]
fn emphasis_is_medium_over_any_kind() {
    for &kind in Type::ALL {
        let mut set = div().typeset(kind).emphasis();

        assert_eq!(set.style().text.font_weight, Some(EMPHASIS), "{kind:?}");
    }
}

/// On a Linux desktop without the font GPUI gives `.SystemUIFont`, text that named it would fall back to a family in
/// its regular face whatever weight it asked for, where the window's family, which the kit's theme names, keeps its
/// medium face.
#[test]
fn only_code_names_a_font_so_the_system_font_is_the_windows() {
    for &kind in Type::ALL {
        let mut set = div().typeset(kind);
        let named = set.style().text.font_family.clone();

        let code = (kind == Type::Code).then(|| SharedString::new_static(MONO));
        assert_eq!(named, code, "{kind:?}");
    }
}

#[test]
fn the_specs_table_has_every_spacing_of_codes_lines_in_order() {
    let table: Vec<[String; 2]> = spec_table::rows("**How far apart code's lines are, by where it's read.**");

    let spaced: Vec<[String; 2]> = CodeLines::ALL.iter().map(|lines| lines.written()).collect();
    assert_eq!(table, spaced, "docs/spec/look.md's table of code's line heights, against crates/look/src/typography");
}
