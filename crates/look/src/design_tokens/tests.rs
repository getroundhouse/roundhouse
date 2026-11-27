use std::collections::{BTreeMap, BTreeSet};

use gpui_kit::{FontWeight, Pixels, phi, px};

use super::reading::{DESIGN, STEPS, scale, tokens};
use crate::chips::INSIDE;
use crate::motion::{FADE, PANEL, SCROLL};
use crate::traits::Reach;
use crate::typography::{CodeLines, INLINE_CODE, LIST_ITEM_GAP, MARKDOWN_HEADINGS, PARAGRAPH_GAP, STRONG, Type, rem};
use crate::{
    BarSize, Colors, ControlSize, Corner, EDGE, IconSize, NamedColor, QUOTE_RULE, RowSize, Space, Strength, TAB_LINE,
};

/// The film of a card opening, whose change is a line such as `#a to(#b) sec(0.2) ease(out,cubic)`.
const FILM: &str = include_str!("../../../../design/Screens/Desktop/Chats/Motion/Card opens.bm");

/// One of the kit's icons, as each of them is drawn but for a few, such as `width="24"` and `stroke-width="2"`.
const ICON: &str = include_str!("../../../../third_party/gpui-kit/vendor/crates/assets/assets/icons/file-text.svg");

/// The groups a test here holds to the app's, each by what its tokens' names start with.
const HELD: &[&str] = &[
    "color",
    "strength",
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

/// The names in the design that a test of their own holds, by their group, rather than their group's: what a chip and
/// a part with an edge have inside, what's between paragraphs and a list's items, inline code's size, and the styles
/// of text that stands out.
const OF_THEIR_OWN: &[(&str, &[&str])] = &[
    (
        "spacing",
        &[
            "Chip",
            "Paragraph",
            "ListItem",
            "EdgedXSmall",
            "EdgedSmall",
            "EdgedMedium",
            "EdgedLarge",
            "ChipStart",
            "ChipEnd",
        ],
    ),
    (
        "font.size",
        &[
            "InlineCode",
            "MarkdownHeading1",
            "MarkdownHeading2",
            "MarkdownHeading3",
            "MarkdownHeading4",
            "MarkdownHeading5",
            "MarkdownHeading6",
        ],
    ),
    ("typography", &["Emphasis", "Strong", "InlineCode"]),
];

/// The tokens in `group` at `reach` that the group's test holds, which is all of them but those [`OF_THEIR_OWN`] names.
fn the_designs(group: &str, reach: Reach) -> BTreeMap<String, String> {
    let only = OF_THEIR_OWN.iter().find(|(named, _)| *named == group).map_or(&[][..], |(_, names)| names);
    let mut tokens = tokens(group, reach);
    tokens.retain(|name, _| !only.contains(&name.as_str()));
    tokens
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
        "Code" => "code",
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
/// a colour of its own or a scale, which what's made from it holds, as a palette's pinned step is, on a line such as
/// `grey.soft { $default: #201b18 }`.
#[test]
fn every_token_but_the_fonts_is_in_a_group_that_is_held_to_the_apps() {
    let known = |name: &str| {
        let after = |group: &str| name.strip_prefix(group).map(|rest| rest.is_empty() || rest.starts_with('.'));
        HELD.iter().chain(THE_FONTS).any(|group| after(group) == Some(true))
    };
    let palettes: BTreeSet<&str> = DESIGN.lines().filter_map(|line| Some(line.split_once(": boldness(")?.0)).collect();
    let named = DESIGN.lines().filter(|line| line.starts_with(|first: char| first.is_ascii_alphabetic()));
    let tokens = named.filter_map(|line| line.split([':', ' ']).next()).filter(|name| name.contains('.'));

    let a_step = |name: &str| name.split('.').next().is_some_and(|palette| palettes.contains(palette));
    let unknown: Vec<_> = tokens.filter(|name| !a_step(name) && !known(name)).collect();
    assert!(unknown.is_empty(), "held to nothing: {unknown:?}");
}

#[test]
fn every_colour_in_the_design_is_the_apps_of_the_same_name() {
    let the_apps = the_apps(Colors::dark().roles().into_iter().map(|role| (in_its_section(&role), role.hex())));
    for reach in Reach::ALL {
        assert_eq!(the_designs("color", reach), the_apps, "{reach:?}");
    }
}

#[test]
fn every_space_in_the_design_is_the_apps_step_in_its_place() {
    let steps = STEPS.iter().zip(Space::ALL);
    let space = the_apps(steps.map(|(step, space)| ((*step).to_string(), points(space.size()))));

    assert_eq!(STEPS.len(), Space::ALL.len());
    assert_eq!(scale("spacing"), space);
    for reach in Reach::ALL {
        let at_a_controls_ends =
            ControlSize::ALL.iter().map(|size| (format!("control.{size:?}"), points(size.padding())));
        assert_eq!(the_designs("spacing", reach), the_apps(at_a_controls_ends), "{reach:?}");
    }
}

/// The lengths beside the scale that the app makes of its own: a chip's, and what a part with an edge has inside, which
/// is its step of space and a pixel more, since its edge takes room in the app and none in the design.
#[test]
fn a_chips_lengths_and_an_edged_parts_are_the_apps() {
    let chip = [("Chip", INSIDE), ("ChipStart", INSIDE + EDGE), ("ChipEnd", Space::Small.size() + EDGE)];
    let edged = [Space::XSmall, Space::Small, Space::Medium, Space::Large].map(|step| (step, step.size() + EDGE));
    let edged = edged.map(|(step, length)| (format!("Edged{step:?}"), length));
    let lengths: Vec<(String, Pixels)> =
        chip.map(|(name, length)| (name.to_string(), length)).into_iter().chain(edged).collect();

    for reach in Reach::ALL {
        for (name, length) in &lengths {
            assert_eq!(tokens("spacing", reach)[name], points(*length), "{name} at {reach:?}");
        }
    }
}

/// How far apart paragraphs and a list's items are, how big inline code is and how big Markdown's headings are, are set
/// by the text they're in, as the app sets them: a paragraph's by a rem, which sets the kit's text, and the rest as
/// shares of the size of the body text they're in.
#[test]
fn what_is_set_by_the_text_is_as_the_app_sets_it() {
    for reach in Reach::ALL {
        let body = Type::Body.size_at(reach, rem(reach));

        assert_eq!(tokens("spacing", reach)["Paragraph"], points(PARAGRAPH_GAP.to_pixels(rem(reach))), "{reach:?}");
        assert_eq!(tokens("spacing", reach)["ListItem"], points(body * LIST_ITEM_GAP), "{reach:?}");
        assert_eq!(tokens("font.size", reach)["InlineCode"], points(body * INLINE_CODE), "{reach:?}");
        for (level, share) in (1..).zip(MARKDOWN_HEADINGS) {
            let drawn: f32 = tokens("font.size", reach)[&format!("MarkdownHeading{level}")].parse().unwrap();
            let apps = f32::from(body * share);
            assert!((drawn - apps).abs() < 0.001, "a heading of level {level} at {reach:?}: {drawn} against {apps}");
        }
    }
}

/// How big an icon is, and how tall a control, a bar and a row are.
#[test]
fn every_size_in_the_design_is_the_apps_of_the_same_name() {
    for reach in Reach::ALL {
        let icons = IconSize::ALL.iter().map(|size| (format!("icon.{size:?}"), size.size_at(reach)));
        let controls = ControlSize::ALL.iter().map(|size| (format!("control.{size:?}"), size.height_at(reach)));
        let bars = BarSize::ALL.iter().map(|size| (format!("bar.{size:?}"), size.height_at(reach)));
        let rows = RowSize::ALL.iter().map(|size| (format!("row.{size:?}"), size.height_at(reach)));
        let sizes = icons.chain(controls).chain(bars).chain(rows).map(|(name, size)| (name, points(size)));

        assert_eq!(the_designs("size", reach), the_apps(sizes), "{reach:?}");
    }
}

/// Every line's thickness: the app's own, and an icon's at each of its sizes, as the kit's icons are drawn.
#[test]
fn every_line_in_the_design_is_as_thick_as_the_apps() {
    let of_the_icon = |named: &str| -> f64 {
        let value = ICON.split(&format!("{named}=\"")).nth(1).and_then(|rest| rest.split('"').next());
        value.expect(named).parse().expect(named)
    };
    for reach in Reach::ALL {
        let icons = IconSize::ALL.iter().map(|size| {
            let thick = f64::from(size.size_at(reach)) * of_the_icon("stroke-width") / of_the_icon("width");
            (format!("icon.{size:?}"), ((thick * 10_000.).round() / 10_000.).to_string())
        });
        let own = [("Edge", EDGE), ("QuoteRule", QUOTE_RULE), ("TabLine", TAB_LINE)];
        let own = own.into_iter().map(|(name, thick)| (name.to_string(), points(thick)));

        assert_eq!(the_designs("stroke.width", reach), the_apps(icons.chain(own)), "{reach:?}");
    }
}

#[test]
fn every_corner_in_the_design_is_the_apps_of_the_same_name() {
    let corner = the_apps(Corner::ALL.iter().map(|corner| (format!("{corner:?}"), points(corner.radius()))));

    for reach in Reach::ALL {
        assert_eq!(tokens("radius", reach), corner, "{reach:?}");
    }
}

#[test]
fn every_kind_of_text_in_the_design_is_as_big_as_the_apps() {
    for reach in Reach::ALL {
        let sizes =
            the_apps(Type::ALL.iter().map(|kind| (format!("{kind:?}"), points(kind.size_at(reach, rem(reach))))));

        assert_eq!(the_designs("font.size", reach), sizes, "{reach:?}");
    }
}

/// How far apart the design's lines are, in points, is the app's: a kind's own, where it has them, or else what it takes
/// where nothing it's in sets them, the golden ratio of its size, drawn to the nearest pixel at two to a point; and
/// code's by where it's read.
#[test]
fn lines_of_text_are_as_far_apart_in_the_design_as_in_the_app() {
    for reach in Reach::ALL {
        let size = |kind: Type| kind.size_at(reach, rem(reach));
        let golden = |kind: Type| px((f32::from(phi().to_pixels(size(kind).into(), rem(reach))) * 2.).round() / 2.);
        let kinds =
            Type::ALL.iter().map(|&kind| (format!("{kind:?}"), kind, kind.line_at(reach).unwrap_or(golden(kind))));
        let code = [("CodeFile", CodeLines::File), ("CodeChat", CodeLines::Chat)]
            .map(|(name, lines)| (name.to_string(), Type::Code, lines.height_at(reach)));
        let lines: Vec<_> = kinds.chain(code).collect();

        let in_the_design = the_designs("font.lineHeight", reach);
        assert_eq!(in_the_design.len(), lines.len(), "{in_the_design:?}");
        for (name, kind, apart) in lines {
            let over_the_size: f32 = in_the_design[&name].parse().unwrap();
            let drawn = over_the_size * f32::from(size(kind));
            let both = format!("{drawn} in the design, {apart:?} in the app");
            assert!((drawn - f32::from(apart)).abs() < 0.01, "{name} at {reach:?}: {both}");
        }
    }
}

/// Each kind of text's style has the app's font and weight for it, and its size, which is the app's as the sizes are.
#[test]
fn every_kind_of_text_has_a_style_in_the_design_as_the_app_sets_it() {
    let kinds: BTreeSet<_> = Type::ALL.iter().map(|kind| format!("{kind:?}")).collect();
    assert_eq!(the_designs("typography", Reach::Desk).into_keys().collect::<BTreeSet<_>>(), kinds);

    for kind in Type::ALL {
        let [name, font, _, weight, _] = kind.written();
        let style = tokens("typography", Reach::Desk).remove(&name).unwrap_or_else(|| panic!("{name} has no style"));
        let part =
            |named: &str| style.split(&format!("{named}: ")).nth(1).and_then(|rest| rest.split([',', ' ']).next());

        assert_eq!(part("fontSize"), Some(format!("font.size.{name}").as_str()), "{name}");
        assert_eq!(part("lineHeight"), Some(format!("font.lineHeight.{name}").as_str()), "{name}");
        let weight = if weight == "Medium" { "font.weight.emphasis" } else { "font.weight.regular" };
        assert_eq!(part("fontWeight"), Some(weight), "{name}");
        assert_eq!(part("fontFamily") == Some("font.family.code"), font == "Code", "{name}'s font, {style}");
    }
}

/// What stands out in body text, and inline code, which the app sets as body text in a weight, a font or a size of its
/// own.
#[test]
fn every_style_of_text_that_stands_out_is_set_as_the_app_sets_it() {
    let body = &tokens("typography", Reach::Desk)["Body"];
    let emphasis = body.replace("font.weight.regular", "font.weight.emphasis");
    let strong = body.replace("font.weight.regular", "font.weight.strong");

    assert_eq!(tokens("typography", Reach::Desk)["Emphasis"], emphasis);
    assert_eq!(tokens("typography", Reach::Desk)["Strong"], strong);
    let code = "{ fontFamily: font.family.code, fontSize: font.size.InlineCode, fontWeight: font.weight.regular }";
    assert_eq!(tokens("typography", Reach::Desk)["InlineCode"], code);
}

/// Emphasis is medium, which is 500 wherever it's drawn. On a Mac the app asks for a little more, so GPUI finds that
/// same face.
#[test]
fn the_weights_in_the_design_are_the_apps() {
    let weights = [("regular", FontWeight::NORMAL), ("emphasis", FontWeight::MEDIUM), ("strong", STRONG)];
    let weights = the_apps(weights.map(|(name, weight)| (name.to_string(), weight.0.to_string())));

    for reach in Reach::ALL {
        assert_eq!(tokens("font.weight", reach), weights, "{reach:?}");
    }
}

/// The one shadow is the app's, layer by layer: how far below, how far it blurs, which Brilliant has as twice what GPUI
/// does, and how strong.
#[test]
fn the_shadow_in_the_design_is_the_apps() {
    let of_the_app = crate::raised::shadow(&Colors::dark()).into_iter().map(|layer| {
        let [below, blur] = [layer.offset.y, layer.blur_radius * 2.].map(f32::from);
        format!("drop(y: {below}, blur: {blur}, color: rgba(0, 0, 0, {}))", layer.color.a)
    });

    assert_eq!(the_designs("shadow", Reach::Desk).into_keys().collect::<Vec<_>>(), ["Raised"]);
    let start = DESIGN.lines().position(|line| line.starts_with("shadow.Raised: [")).expect("the shadow");
    let drawn = DESIGN.lines().skip(start + 1).take_while(|line| *line != "]");
    let layers: Vec<_> = drawn.map(|line| line.trim().trim_end_matches(',').to_string()).collect();
    assert_eq!(layers, of_the_app.collect::<Vec<_>>());
}

/// Each strength a role is drawn at is one of the app's steps, by its name, as `f32` writes its share.
#[test]
fn every_strength_in_the_design_is_the_apps_of_the_same_name() {
    let strengths = the_apps(Strength::ALL.iter().map(|step| (format!("{step:?}"), step.share().to_string())));

    for reach in Reach::ALL {
        assert_eq!(tokens("strength", reach), strengths, "{reach:?}");
    }
}

#[test]
fn what_moves_takes_as_long_in_the_design_as_in_the_app() {
    let lengths = [("Panel", PANEL), ("Fade", FADE), ("Scroll", SCROLL)];
    let lengths = lengths.map(|(name, length)| (name.to_string(), length.as_millis().to_string()));

    for reach in Reach::ALL {
        assert_eq!(the_designs("motion", reach), the_apps(lengths.clone()), "{reach:?}");
    }
}

/// A film's change takes as long as the app's panel does, and slows toward its end as the app's does.
#[test]
fn a_films_change_takes_as_long_as_the_apps_panel_and_is_eased_as_it_is() {
    let change = FILM.lines().find(|line| line.contains(" to(#b) ")).expect("the film's change");

    let seconds: f64 = change.split("sec(").nth(1).unwrap().split(')').next().unwrap().parse().unwrap();
    assert_eq!(std::time::Duration::from_secs_f64(seconds), PANEL);
    assert!(change.contains("ease(out,cubic)"), "{change}");
}
