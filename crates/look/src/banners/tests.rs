use super::BannerKind;
use crate::colors::Colors;
use crate::spec_table;

#[test]
fn the_specs_table_has_every_kind_of_banner_in_order() {
    let table: Vec<[String; 2]> = spec_table::rows("**Every kind of banner, by its name.**");

    let kinds: Vec<[String; 2]> = BannerKind::ALL.iter().map(|kind| kind.written()).collect();
    assert_eq!(table, kinds, "docs/spec/components.md's table of kinds of banner, against crates/look/src/banners");
}

/// Each kind is washed in a colour of its own, so an error, a warning and a note are told apart at a glance.
#[test]
fn each_kind_of_banner_has_a_colour_of_its_own() {
    let colors = Colors::dark();

    assert_eq!(BannerKind::Error.tone(&colors), colors.error);
    assert_eq!(BannerKind::Warning.tone(&colors), colors.warning);
    assert_eq!(BannerKind::Note.tone(&colors), colors.text_muted);
}
