use gpui_kit::hsla;

use super::BadgeKind;
use super::label::readable;
use crate::spec_table;

#[test]
fn the_specs_table_has_every_kind_of_badge_in_order() {
    let table: Vec<[String; 1]> = spec_table::rows("**Every kind of badge, by its name.**");

    let kinds: Vec<[String; 1]> = BadgeKind::ALL.iter().map(|kind| kind.written()).collect();
    assert_eq!(table, kinds, "docs/spec/components.md's table of kinds of badge, against crates/look/src/badges");
}

/// A pale label and a dark one read alike, at the lightness their words are drawn at, in their own hue.
#[test]
fn a_labels_words_are_as_light_whatever_its_colour() {
    let pale = readable(hsla(0.15, 0.97, 0.5, 1.));
    let dark = readable(hsla(0.0, 0.9, 0.36, 1.));

    assert!((0.72..=0.82).contains(&pale.l) && (0.72..=0.82).contains(&dark.l), "{pale:?} {dark:?}");
    assert!(pale.s <= 0.75 && dark.s <= 0.75, "{pale:?} {dark:?}");
    assert!((pale.h - 0.15).abs() < f32::EPSILON, "its own hue");
}
