//! The design system's tokens, which Brilliant's canvases name. A canvas whose token the design system no longer has is
//! still drawn, with the value the token last had, and nothing says so. So a merge of two branches, one removing a token
//! and the other using it, reads as fine. And Brilliant makes nine steps of a palette, which can be drawn with whether or
//! not anyone chose them, so a canvas draws with roles alone.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Every canvas under `folder`, but for what Brilliant keeps for itself, in folders that start with a dot.
fn canvases(folder: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(folder).expect("the design folder") {
        let path = entry.expect("an entry").path();
        let hidden = path.file_name().is_some_and(|name| name.to_string_lossy().starts_with('.'));
        if hidden {
            continue;
        }
        if path.is_dir() {
            canvases(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "bl") {
            found.push(path);
        }
    }
}

/// The names `marker` is followed by in `canvas`, up to the first character a token's name can't have.
fn named_after(canvas: &str, marker: &str) -> Vec<String> {
    canvas
        .split(marker)
        .skip(1)
        .map(|after| after.chars().take_while(|c| c.is_alphanumeric() || "._".contains(*c)).collect::<String>())
        .map(|name| name.trim_end_matches('.').to_string())
        .filter(|name| !name.is_empty())
        .collect()
}

/// The names that a canvas uses: `tok(color.raised)` in a colour and `$space.Small` in a size.
fn used(canvas: &str) -> BTreeSet<String> {
    ["tok(", "$"].into_iter().flat_map(|marker| named_after(canvas, marker)).collect()
}

/// What Brilliant names a scale's steps, from the smallest.
const STEPS: [&str; 6] = ["xs", "sm", "md", "lg", "xl", "2xl"];

/// The names the design system has: `color.text.faint` for a line `color.text.faint: #7c7671`, and, for a line such as
/// `spacing: tshirt(number([4, 8]), min: { none: 0 })`, a step for each of its numbers, and the step it starts at.
/// Brilliant makes more steps of some scales than they're given, such as `radius.xl`, and those aren't the design's.
fn defined(system: &str) -> BTreeSet<String> {
    let lines = system.lines().filter_map(|line| line.split_once(": "));
    lines
        .flat_map(|(name, value)| {
            let name = name.trim();
            let listed = value.strip_prefix("tshirt(number([").and_then(|rest| rest.split_once("])"));
            let steps = listed.map_or(0, |(values, _)| values.split(',').count());
            let none = listed.is_some_and(|(_, rest)| rest.contains("none: 0")).then_some("none");
            let named = STEPS.into_iter().take(steps).chain(none).map(move |step| format!("{name}.{step}"));
            std::iter::once(name.to_string()).chain(named.collect::<Vec<_>>())
        })
        .collect()
}

#[test]
fn a_scale_has_a_step_for_each_number_it_is_given_and_no_other() {
    let names = defined("radius: tshirt(number([2, 4]))\nspacing: tshirt(number([4]), min: { none: 0 })");

    assert_eq!(
        names,
        ["radius", "radius.xs", "radius.sm", "spacing", "spacing.xs", "spacing.none"].map(String::from).into()
    );
}

/// Every canvas of the design, as the path it's shown by and its text.
fn the_designs_canvases() -> Vec<(String, String)> {
    let design = design();
    let mut found = Vec::new();
    canvases(&design, &mut found);
    found
        .into_iter()
        .map(|path| {
            let file = path.strip_prefix(&design).expect("a path in it").display().to_string();
            (file, fs::read_to_string(&path).expect("a canvas"))
        })
        .collect()
}

fn design() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../design")
}

/// The colours a canvas draws with by name that aren't roles, such as a palette's step: `tok(grey.soft,#201b18)`.
fn colours_that_are_not_roles(canvas: &str) -> BTreeSet<String> {
    named_after(canvas, "tok(").into_iter().filter(|name| !name.starts_with("color.")).collect()
}

#[test]
fn a_palettes_step_and_a_colour_of_its_own_are_not_roles() {
    let canvas = "f[(1,tok(color.text,#eee6e1)),(2,tok(grey.soft,#201b18)),(3,tok(amber,#e9b24c))] $spacing.sm";

    assert_eq!(colours_that_are_not_roles(canvas), ["amber", "grey.soft"].map(String::from).into());
}

#[test]
fn a_canvas_draws_with_roles_rather_than_with_the_colours_they_are_made_from() {
    let wrong: Vec<String> = the_designs_canvases()
        .iter()
        .flat_map(|(file, text)| {
            colours_that_are_not_roles(text).into_iter().map(move |name| format!("{file}: {name}"))
        })
        .collect();

    assert!(wrong.is_empty(), "draw with a role, a `color.` token of design/Styles/default.ds: {wrong:#?}");
}

#[test]
fn no_canvas_names_a_token_the_design_system_does_not_have() {
    let names = defined(&fs::read_to_string(design().join("Styles/default.ds")).expect("the design system"));

    let missing: Vec<String> = the_designs_canvases()
        .iter()
        .flat_map(|(file, text)| {
            let missing = used(text).into_iter().filter(|name| !names.contains(name));
            missing.map(move |name| format!("{file}: {name}")).collect::<Vec<_>>()
        })
        .collect();

    assert!(missing.is_empty(), "tokens that design/Styles/default.ds doesn't have: {missing:#?}");
}
