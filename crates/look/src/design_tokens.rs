//! Holds the design system in `design/Styles/default.ds` to the app's values of the same names, both ways, so a colour,
//! a space, a corner or a kind of text can't be one thing in the design and another in the app.

use std::collections::{BTreeMap, BTreeSet};

use gpui_kit::{FontWeight, Pixels, phi, px};

use crate::cards::WASH;
use crate::chips::INSIDE;
use crate::motion::{FADE, PANEL, SCROLL};
use crate::traits::Reach;
use crate::typography::{CodeLines, INLINE_CODE, LIST_ITEM_GAP, STRONG, Type, rem};
use crate::{BarSize, Colors, ControlSize, Corner, EDGE, IconSize, NamedColor, QUOTE_RULE, RowSize, Space, TAB_LINE};

/// The design system, whose tokens are lines such as `color.surface.prompt: #36251a`.
const DESIGN: &str = include_str!("../../../design/Styles/default.ds");

/// The film of a card opening, whose change is a line such as `#a to(#b) sec(0.2) ease(out,cubic)`.
const FILM: &str = include_str!("../../../design/Motion/Card opens.bm");

/// One of the kit's icons, as each of them is drawn but for a few, such as `width="24"` and `stroke-width="2"`.
const ICON: &str = include_str!("../../../third_party/gpui-kit/vendor/crates/assets/assets/icons/file-text.svg");

/// The groups a test here holds to the app's, each by what its tokens' names start with.
const HELD: &[&str] = &[
    "color",
    "spacing",
    "radius",
    "size",
    "stroke.width",
    "shadow",
    "motion",
    "font.weight",
    "font.size",
    "font.lineHeight",
    "typography",
];

/// The groups the app has no values for: the font Brilliant draws the system font with, and the spacing that makes it as
/// wide, which macOS gives the system font.
const THE_FONTS: &[&str] = &["font.family", "font.letterSpacing"];

/// The names in the design that a test of their own holds, by their group, rather than their group's: the wash of a
/// card, what a chip, a part with an edge and a list have inside, inline code's size, and the styles of text that
/// stands out.
const OF_THEIR_OWN: &[(&str, &[&str])] = &[
    ("color", &["status.needs_you_wash"]),
    ("spacing", &["Chip", "ListItem", "Edged", "ChipStart", "ChipEnd"]),
    ("font.size", &["InlineCode"]),
    ("typography", &["Emphasis", "Strong", "InlineCode"]),
];

/// The tokens in `group` that the group's test holds, which is all of them but those [`OF_THEIR_OWN`] names.
fn the_designs(group: &str) -> BTreeMap<String, String> {
    let only = OF_THEIR_OWN.iter().find(|(named, _)| *named == group).map_or(&[][..], |(_, names)| names);
    let mut tokens = tokens(group);
    tokens.retain(|name, _| !only.contains(&name.as_str()));
    tokens
}

/// The tokens in the design whose names start with `group` and a dot, by the rest of their names.
fn tokens(group: &str) -> BTreeMap<String, String> {
    let prefix = format!("{group}.");
    DESIGN
        .lines()
        .filter_map(|line| line.strip_prefix(&prefix)?.split_once(": "))
        .map(|(name, value)| (name.to_string(), resolved(value.trim())))
        .collect()
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
const STEPS: [&str; 6] = ["xs", "sm", "md", "lg", "xl", "2xl"];

/// The steps of the scale `name`, on a line such as `spacing: tshirt(number([4, 8]))`, by what Brilliant names them.
fn scale(name: &str) -> BTreeMap<String, String> {
    let start = format!("{name}: tshirt(number([");
    let listed = DESIGN.lines().find_map(|line| line.strip_prefix(start.as_str())?.split_once("])"));
    let values = listed.map_or_default(|(values, _)| values.split(", ").collect::<Vec<_>>());
    assert!(values.len() <= STEPS.len(), "{name} has more steps than Brilliant's names this knows");
    STEPS.iter().zip(values).map(|(step, value)| ((*step).to_string(), value.to_string())).collect()
}

/// What the design calls `role`: the section its group is, which is how Brilliant's page of roles is divided, and its
/// name, without the section's where it starts with it.
fn in_its_section(role: &NamedColor) -> String {
    let section = match role.group {
        "Surfaces" => "surface",
        "Under the pointer, pressed or chosen" => "pointer",
        "Edges and the shadow" => "edge",
        "Text" => "text",
        "Buttons" => "button",
        "The brand" => "brand",
        "What happened" => "status",
        "A pull request" => "PR",
        "The window" => "window",
        group => panic!("{group} has no section in the design"),
    };
    // Brilliant heads a section with its name, so this one's is in capitals there.
    match role.name.strip_prefix(&section.to_lowercase()) {
        Some("") => section.to_string(),
        Some(rest) if rest.starts_with('_') => format!("{section}.{}", &rest[1..]),
        _ => format!("{section}.{}", role.name),
    }
}

/// `length`, as the design writes it.
fn points(length: Pixels) -> String {
    f32::from(length).to_string()
}

/// What `named` gives each of the names in the design, by its name, as the app has it.
fn the_apps(named: impl IntoIterator<Item = (String, String)>) -> BTreeMap<String, String> {
    named.into_iter().collect()
}

/// A token in a group that nothing here knows isn't held to anything, so it's refused. A name with no dot is a palette,
/// a colour of its own or a scale, which what's made from it holds.
#[test]
fn every_token_but_the_fonts_is_in_a_group_that_is_held_to_the_apps() {
    let known = |name: &str| {
        let after = |group: &str| name.strip_prefix(group).map(|rest| rest.is_empty() || rest.starts_with('.'));
        HELD.iter().chain(THE_FONTS).any(|group| after(group) == Some(true))
    };
    let named = DESIGN.lines().filter(|line| line.starts_with(|first: char| first.is_ascii_alphabetic()));
    let tokens = named.filter_map(|line| Some(line.split_once(": ")?.0)).filter(|name| name.contains('.'));

    // A pinned step of a palette, on a line such as `grey.soft { $default: #201b18 }`, is held by what names it.
    let unknown: Vec<_> = tokens.filter(|name| !name.contains('{') && !known(name)).collect();
    assert!(unknown.is_empty(), "held to nothing: {unknown:?}");
}

#[test]
fn every_colour_in_the_design_is_the_apps_of_the_same_name() {
    let the_apps = the_apps(Colors::dark().roles().into_iter().map(|role| (in_its_section(&role), role.hex())));
    assert_eq!(the_designs("color"), the_apps);
}

#[test]
fn every_space_in_the_design_is_the_apps_step_in_its_place() {
    let steps = STEPS.iter().zip(Space::ALL);
    let space = the_apps(steps.map(|(step, space)| ((*step).to_string(), points(space.size()))));

    assert_eq!(STEPS.len(), Space::ALL.len());
    assert_eq!(scale("spacing"), space);
    let at_a_controls_ends =
        ControlSize::ALL.iter().map(|size| (format!("control.{size:?}"), points(size.padding_at(Reach::Desk))));
    assert_eq!(the_designs("spacing"), the_apps(at_a_controls_ends));
}

/// The lengths beside the scale that the app makes of its own: a chip's, and what a part with an edge has inside, which
/// is a pixel more than the app's, since its edge takes room in the app and none in the design.
#[test]
fn a_chips_lengths_and_an_edged_parts_are_the_apps() {
    let small = Space::Small.size();
    let large = Space::Large.size();
    let lengths = [("Chip", INSIDE), ("ChipStart", INSIDE + EDGE), ("ChipEnd", small + EDGE), ("Edged", large + EDGE)];

    for (name, length) in lengths {
        assert_eq!(tokens("spacing")[name], points(length), "{name}");
    }
}

/// How far apart a list's items are, and how big inline code is, are the app's shares of the size of the body text
/// they're in.
#[test]
fn what_is_a_share_of_body_text_is_the_apps_share_of_it() {
    let body = Type::Body.size(rem(Reach::Desk));

    assert_eq!(tokens("spacing")["ListItem"], points(body * LIST_ITEM_GAP));
    assert_eq!(tokens("font.size")["InlineCode"], points(body * INLINE_CODE));
}

/// How big an icon is, and how tall a control, a bar and a row are, at a desk.
#[test]
fn every_size_in_the_design_is_the_apps_of_the_same_name() {
    let icons = IconSize::ALL.iter().map(|size| (format!("icon.{size:?}"), size.size_at(Reach::Desk)));
    let controls = ControlSize::ALL.iter().map(|size| (format!("control.{size:?}"), size.height_at(Reach::Desk)));
    let bars = BarSize::ALL.iter().map(|size| (format!("bar.{size:?}"), size.height_at(Reach::Desk)));
    let rows = RowSize::ALL.iter().map(|size| (format!("row.{size:?}"), size.height_at(Reach::Desk)));
    let sizes = icons.chain(controls).chain(bars).chain(rows);

    assert_eq!(the_designs("size"), the_apps(sizes.map(|(name, size)| (name, points(size)))));
}

/// Every line's thickness: the app's own, and an icon's at each of its sizes, as the kit's icons are drawn.
#[test]
fn every_line_in_the_design_is_as_thick_as_the_apps() {
    let of_the_icon = |named: &str| -> f64 {
        let value = ICON.split(&format!("{named}=\"")).nth(1).and_then(|rest| rest.split('"').next());
        value.expect(named).parse().expect(named)
    };
    let icons = IconSize::ALL.iter().map(|size| {
        let thick = f64::from(size.size_at(Reach::Desk)) * of_the_icon("stroke-width") / of_the_icon("width");
        (format!("icon.{size:?}"), ((thick * 10_000.).round() / 10_000.).to_string())
    });
    let own = [("Edge", EDGE), ("QuoteRule", QUOTE_RULE), ("TabLine", TAB_LINE)];
    let own = own.into_iter().map(|(name, thick)| (name.to_string(), points(thick)));

    assert_eq!(the_designs("stroke.width"), the_apps(icons.chain(own)));
}

#[test]
fn every_corner_in_the_design_is_the_apps_of_the_same_name() {
    let corner = the_apps(Corner::ALL.iter().map(|corner| (format!("{corner:?}"), points(corner.radius()))));

    assert_eq!(tokens("radius"), corner);
}

#[test]
fn every_kind_of_text_in_the_design_is_as_big_as_the_apps() {
    let sizes = the_apps(
        Type::ALL.iter().map(|kind| (format!("{kind:?}"), kind.written()[2].trim_end_matches("px").to_string())),
    );

    assert_eq!(the_designs("font.size"), sizes);
}

/// How far apart the design's lines are, in points, is the app's: body text's is the golden ratio of its size, drawn to
/// the nearest pixel at two to a point.
#[test]
fn lines_of_text_are_as_far_apart_in_the_design_as_in_the_app() {
    let rem = rem(Reach::Desk);
    let size = |kind: Type| kind.size(rem);
    let body = px((f32::from(phi().to_pixels(size(Type::Body).into(), rem)) * 2.).round() / 2.);
    let of_its_own = |kind: Type| kind.line(rem).expect("lines of its own");
    let lines = [
        ("Body", Type::Body, body),
        ("CodeFile", Type::Code, CodeLines::File.height()),
        ("CodeChat", Type::Code, CodeLines::Chat.height()),
        ("Key", Type::Key, of_its_own(Type::Key)),
        ("KeyHeld", Type::KeyHeld, of_its_own(Type::KeyHeld)),
    ];

    let in_the_design = the_designs("font.lineHeight");
    assert_eq!(in_the_design.len(), lines.len(), "{in_the_design:?}");
    for (name, kind, apart) in lines {
        let over_the_size: f32 = in_the_design[name].parse().unwrap();
        let drawn = over_the_size * f32::from(size(kind));
        assert!((drawn - f32::from(apart)).abs() < 0.01, "{name}: {drawn} in the design, {apart:?} in the app");
    }
}

/// Each kind of text's style has the app's font and weight for it, and its size, which is the app's as the sizes are.
#[test]
fn every_kind_of_text_has_a_style_in_the_design_as_the_app_sets_it() {
    let kinds: BTreeSet<_> = Type::ALL.iter().map(|kind| format!("{kind:?}")).collect();
    assert_eq!(the_designs("typography").into_keys().collect::<BTreeSet<_>>(), kinds);

    for kind in Type::ALL {
        let [name, font, _, weight, _] = kind.written();
        let style = tokens("typography").remove(&name).unwrap_or_else(|| panic!("{name} has no style"));
        let part =
            |named: &str| style.split(&format!("{named}: ")).nth(1).and_then(|rest| rest.split([',', ' ']).next());

        assert_eq!(part("fontSize"), Some(format!("font.size.{name}").as_str()), "{name}");
        let weight = if weight == "Medium" { "font.weight.emphasis" } else { "font.weight.regular" };
        assert_eq!(part("fontWeight"), Some(weight), "{name}");
        assert_eq!(part("fontFamily") == Some("font.family.code"), font == "Code", "{name}'s font, {style}");
    }
}

/// What stands out in body text, and inline code, which the app sets as body text in a weight, a font or a size of its
/// own.
#[test]
fn every_style_of_text_that_stands_out_is_set_as_the_app_sets_it() {
    let body = &tokens("typography")["Body"];
    let emphasis = body.replace("font.weight.regular", "font.weight.emphasis");
    let strong = body.replace("font.weight.regular", "font.weight.strong");

    assert_eq!(tokens("typography")["Emphasis"], emphasis);
    assert_eq!(tokens("typography")["Strong"], strong);
    let code = "{ fontFamily: font.family.code, fontSize: font.size.InlineCode, fontWeight: font.weight.regular }";
    assert_eq!(tokens("typography")["InlineCode"], code);
}

/// Emphasis is medium, which is 500 wherever it's drawn. On a Mac the app asks for a little more, so GPUI finds that
/// same face.
#[test]
fn the_weights_in_the_design_are_the_apps() {
    let weights = [("regular", FontWeight::NORMAL), ("emphasis", FontWeight::MEDIUM), ("strong", STRONG)];
    let weights = the_apps(weights.map(|(name, weight)| (name.to_string(), weight.0.to_string())));

    assert_eq!(tokens("font.weight"), weights);
}

/// The one shadow is the app's, layer by layer: how far below, how far it blurs, which Brilliant has as twice what GPUI
/// does, and how strong.
#[test]
fn the_shadow_in_the_design_is_the_apps() {
    let of_the_app = crate::raised::shadow(&Colors::dark()).into_iter().map(|layer| {
        let [below, blur] = [layer.offset.y, layer.blur_radius * 2.].map(f32::from);
        format!("drop(y: {below}, blur: {blur}, color: rgba(0, 0, 0, {}))", layer.color.a)
    });

    assert_eq!(the_designs("shadow").into_keys().collect::<Vec<_>>(), ["Raised"]);
    let start = DESIGN.lines().position(|line| line.starts_with("shadow.Raised: [")).expect("the shadow");
    let drawn = DESIGN.lines().skip(start + 1).take_while(|line| *line != "]");
    let layers: Vec<_> = drawn.map(|line| line.trim().trim_end_matches(',').to_string()).collect();
    assert_eq!(layers, of_the_app.collect::<Vec<_>>());
}

/// The plan card's wash is the colour that needs you, at the strength the app washes it with.
#[test]
fn the_wash_is_needs_you_at_the_apps_strength() {
    let wash = Colors::dark().needs_you.opacity(WASH);

    assert_eq!(tokens("color")["status.needs_you_wash"], crate::theme::hex(wash));
}

#[test]
fn what_moves_takes_as_long_in_the_design_as_in_the_app() {
    let lengths = [("Panel", PANEL), ("Fade", FADE), ("Scroll", SCROLL)];
    let lengths = lengths.map(|(name, length)| (name.to_string(), length.as_millis().to_string()));

    assert_eq!(the_designs("motion"), the_apps(lengths));
}

/// A film's change takes as long as the app's panel does, and slows toward its end as the app's does.
#[test]
fn a_films_change_takes_as_long_as_the_apps_panel_and_is_eased_as_it_is() {
    let change = FILM.lines().find(|line| line.contains(" to(#b) ")).expect("the film's change");

    let seconds: f64 = change.split("sec(").nth(1).unwrap().split(')').next().unwrap().parse().unwrap();
    assert_eq!(std::time::Duration::from_secs_f64(seconds), PANEL);
    assert!(change.contains("ease(out,cubic)"), "{change}");
}
