use gpui_kit::{FontWeight, SharedString, Styled as _, div};

use super::{MONO, Type, Typeset as _};
use crate::spec_table;

#[test]
fn the_specs_table_has_every_kind_of_text_in_order_as_the_app_sets_it() {
    let table: Vec<[String; 4]> = spec_table::rows("**Every kind of text, by its name.**");

    let set: Vec<[String; 4]> = Type::ALL.iter().map(|kind| kind.written()).collect();
    assert_eq!(table, set, "docs/spec/look.md's table of the kinds of text, against crates/look/src/typography.rs");
}

#[test]
fn each_kind_sets_its_size_and_its_weight() {
    for &kind in Type::ALL {
        let mut set = div().typeset(kind);
        let text = &set.style().text;

        assert_eq!(text.font_size, Some(kind.set().size), "{kind:?}");
        assert_eq!(text.font_weight, kind.set().weight, "{kind:?}");
    }
}

#[test]
fn emphasis_is_medium_over_any_kind() {
    for &kind in Type::ALL {
        let mut set = div().typeset(kind).emphasis();

        assert_eq!(set.style().text.font_weight, Some(FontWeight::MEDIUM), "{kind:?}");
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
