//! How the design system's file is read: its tokens by their names, each at a reach, and the steps of a scale.

use std::collections::BTreeMap;

use crate::traits::Reach;

/// The design system, whose tokens are lines such as `color.surface.prompt: #36251a`.
pub(super) const DESIGN: &str = include_str!("../../../../design/Styles/default.ds");

/// The tokens in the design whose names start with `group` and a dot, by the rest of their names, at `reach`: on a line
/// such as `size.icon.Small: 12`, the same at both, and on one such as `size.icon.Small { $default: 12,
/// reach.handheld: 14 }`, the default at a desk and the handheld's on a handheld.
pub(super) fn tokens(group: &str, reach: Reach) -> BTreeMap<String, String> {
    let prefix = format!("{group}.");
    let named = DESIGN.lines().filter_map(|line| {
        let rest = line.strip_prefix(&prefix)?;
        match rest.split_once(" { ") {
            Some((name, modes)) if !name.contains(':') => Some((name, at(modes.strip_suffix(" }")?, reach)?)),
            _ => rest.split_once(": "),
        }
    });
    named.map(|(name, value)| (name.to_string(), resolved(value.trim()))).collect()
}

/// The value at `reach` of a token's `modes`, such as `$default: 12, reach.handheld: 14`: its own, or the default.
fn at(modes: &str, reach: Reach) -> Option<&str> {
    let mode = |key: &str| modes.split(", ").find_map(|mode| mode.strip_prefix(key));
    let own = match reach {
        Reach::Desk => None,
        Reach::Handheld => mode("reach.handheld: "),
    };
    own.or_else(|| mode("$default: "))
}

/// What `value` is: itself, the colour it names, such as `amber`, or the colour of the palette's step it names, such as
/// `grey.soft`. A step that isn't pinned, on a line such as `grey.soft { $default: #201b18 }`, is only known here if it's
/// `mid`, which is the colour the palette is made from.
fn resolved(value: &str) -> String {
    let after = |start: String, end: &str| {
        DESIGN.lines().find_map(|line| Some(line.strip_prefix(start.as_str())?.strip_suffix(end)?.to_string()))
    };
    let pinned = after(format!("{value} {{ $default: "), " }");
    let made_from = value.strip_suffix(".mid").and_then(|palette| after(format!("{palette}: boldness(color("), "))"));
    let of_its_own = after(format!("{value}: color("), ")");
    let on_a_scale = value.split_once('.').and_then(|(name, step)| scale(name).get(step).cloned());
    pinned.or(made_from).or(of_its_own).or(on_a_scale).unwrap_or_else(|| value.to_string())
}

/// The steps Brilliant names on a scale, from the smallest.
pub(super) const STEPS: [&str; 6] = ["xs", "sm", "md", "lg", "xl", "2xl"];

/// The steps of the scale `name`, on a line such as `spacing: tshirt(number([4, 8]))`, by what Brilliant names them.
pub(super) fn scale(name: &str) -> BTreeMap<String, String> {
    let start = format!("{name}: tshirt(number([");
    let listed = DESIGN.lines().find_map(|line| line.strip_prefix(start.as_str())?.split_once("])"));
    let values = listed.map_or_default(|(values, _)| values.split(", ").collect::<Vec<_>>());
    assert!(values.len() <= STEPS.len(), "{name} has more steps than Brilliant's names this knows");
    STEPS.iter().zip(values).map(|(step, value)| ((*step).to_string(), value.to_string())).collect()
}
