//! The design system's tokens, which Brilliant's canvases name. A canvas whose token the design system no longer has is
//! still drawn, with the value the token last had, and nothing says so. So a merge of two branches, one removing a token
//! and the other using it, reads as fine. And Brilliant makes nine steps of a palette, which can be drawn with whether or
//! not anyone chose them, so a canvas draws with roles alone. A foundation's board draws every token it shows, so one
//! added to the design system is seen where it's looked for.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

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

/// The names the design system has: `color.text.faint` for a line `color.text.faint: #7c7671`, `font.size.Body` for one
/// with a value for each mode, `font.size.Body { $default: 14, reach.handheld: 17 }`, and, for a line such as
/// `spacing: tshirt(number([4, 8]), min: { none: 0 })`, a step for each of its numbers, and the step it starts at.
/// Brilliant makes more steps of some scales than they're given, such as `radius.xl`, and those aren't the design's.
fn defined(system: &str) -> BTreeSet<String> {
    let lines = system.lines().filter_map(|line| match line.split_once(" { ") {
        Some((name, modes)) if !name.contains(':') => Some((name, modes)),
        _ => line.split_once(": "),
    });
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

#[test]
fn a_token_with_a_value_for_each_mode_is_named_once() {
    let names =
        defined("font.size.Body { $default: 14, reach.handheld: 17 }\ntypography.Body: { fontSize: font.size.Body }");

    assert_eq!(names, ["font.size.Body", "typography.Body"].map(String::from).into());
}

/// Every canvas of the design, as the path it's shown by and its text.
fn the_designs_canvases() -> Vec<(String, String)> {
    let canvases = design::canvases(&design()).expect("the design's canvases");
    canvases.into_iter().map(|canvas| (format!("{}.bl", canvas.path), canvas.text)).collect()
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

/// Each foundation's board, and how the names of the tokens it shows begin. It draws every one of them in its frame named
/// `Values`, as `docs/standards/design.md` says. Its other frames, such as the heading and the rail, draw with some of
/// the same tokens, so what they draw isn't counted.
const BOARDS: [(&str, &[&str]); 3] = [
    ("Foundations/Colours.bl", &["color.", "strength.", "shadow."]),
    ("Foundations/Type.bl", &["font.", "typography."]),
    ("Foundations/Space and shape.bl", &["spacing.", "radius.", "stroke.width.", "size."]),
];

/// The rows of the frame in `canvas` named `name`: its own, and the ones under it, which are indented further.
fn frame(canvas: &str, name: &str) -> Option<String> {
    let indent = |line: &str| line.len() - line.trim_start().len();
    let quoted = format!("\"{name}\"");
    let mut lines = canvas.lines().skip_while(|line| !line.trim_end().ends_with(&quoted));
    let first = lines.next()?;
    let under = lines.take_while(|line| indent(line) > indent(first));
    Some(std::iter::once(first).chain(under).collect::<Vec<_>>().join("\n"))
}

#[test]
fn a_frame_is_its_row_and_the_rows_under_it() {
    let canvas = "1 al(v) \"Board\"\n  2 al(v) \"Values\"\n    3 r f[(4,tok(color.text,#eee6e1))]\n  5 al(v) \"Rail\"";

    assert_eq!(
        frame(canvas, "Values").as_deref(),
        Some("  2 al(v) \"Values\"\n    3 r f[(4,tok(color.text,#eee6e1))]")
    );
    assert_eq!(frame(canvas, "Top"), None);
}

#[test]
fn a_foundations_board_draws_every_token_it_shows_among_its_values() {
    let names = defined(&fs::read_to_string(design().join("Styles/default.ds")).expect("the design system"));
    let canvases = the_designs_canvases();

    let missing: Vec<String> = BOARDS
        .iter()
        .flat_map(|&(board, shown)| {
            let (_, text) = canvases.iter().find(|(file, _)| file == board).expect("the board's canvas");
            let drawn = used(&frame(text, "Values").expect("the board's frame named Values"));
            let missing = names.iter().filter(|name| shown.iter().any(|start| name.starts_with(start)));
            missing.filter(|name| !drawn.contains(*name)).map(|name| format!("{board}: {name}")).collect::<Vec<_>>()
        })
        .collect();

    assert!(missing.is_empty(), "draw each in its board's frame named Values: {missing:#?}");
}
