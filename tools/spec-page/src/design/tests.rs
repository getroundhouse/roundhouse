use std::path::Path;

use crate::index::{Area, Index, Place, Size, area, read};

use super::html::{COLOURS, COMPONENTS, FOUNDATIONS, STYLE, all_named, colours, text};
use super::{draw, write};

/// The area from `text`, with every screen 400 by 200 pixels, and `places` beside the one named `spaces`.
fn foundations(text: &str, places: &[(&str, Place)]) -> Area {
    let mut area = area("look.md", "The foundations", text).unwrap();
    let blocks = area.sections.iter_mut().flat_map(|section| &mut section.blocks);
    for screen in blocks.flat_map(|block| &mut block.screens) {
        screen.size = Some(Size { width: 400, height: 200 });
        if screen.name.as_str() == "spaces" {
            let named = places.iter().map(|(name, place)| ((*name).to_string(), *place)).collect();
            screen.places.insert("look::Space".into(), named);
        }
    }
    area
}

/// The components, with a section of rules and no names.
fn components() -> Area {
    let text = "# Components\n\nThe pieces screens are built from.\n\n## Buttons\n\n- **One main action an area.**\n";
    area("components.md", "Components", text).unwrap()
}

const SPACES: &str = "# Look\n\n## Space\n\n- **Every space is a step.**\n\n\
                      **Every step, by its name.** Each is a step in `look::Space`. *Unconfirmed.*\n\n\
                      | Name | Room | For |\n|---|---|---|\n| `Small` | 8px | Between an icon and its words |\n\
                      | `Medium` | 12px | Around what's in a card |\n| `Snug` | 8px | Between buttons |\n\n\
                      **Every step, as the app leaves it.**\n\n![Every step](screens/spaces.png)\n\n\
                      ## The pointer\n\n- **A hand over what can be pressed.**\n";

#[test]
fn each_name_shows_its_piece_of_its_types_sheet_at_its_size_in_the_app() {
    let place = Place { x: 24, y: 40, width: 120, height: 32 };

    let page = draw(&foundations(SPACES, &[("Small", place)]), &components()).into_string();

    let cut = "<span class=\"cut\" role=\"img\" aria-label=\"Small\" style=\"width: 60px; height: 16px; \
               background-image: url('screens/spaces.png'); background-size: 200px 100px; \
               background-position: -12px -20px\"></span>";
    assert!(page.contains(cut), "{page}");
    assert!(page.contains("<tr class=\"name\" id=\"Space-Small\""), "{page}");
    assert!(page.contains("<a href=\"#Space-Small\"><code>Small</code></a>"), "{page}");
    assert!(page.contains("<td class=\"value\">8px</td>"), "{page}");
}

#[test]
fn a_name_its_sheet_has_no_place_for_says_it_has_no_picture() {
    let page = draw(&foundations(SPACES, &[]), &components()).into_string();

    let row = page.split("id=\"Space-Medium\"").nth(1).unwrap();
    assert!(row.split("</tr>").next().unwrap().contains("No picture"), "{page}");
}

#[test]
fn names_with_the_same_value_say_so() {
    let page = draw(&foundations(SPACES, &[]), &components()).into_string();

    let row = |name: &str| {
        let after = page.split(&format!("id=\"Space-{name}\"")).nth(1).unwrap();
        after.split("</tr>").next().unwrap().to_string()
    };
    assert!(row("Small").contains("Same as <code>Snug</code>"), "{page}");
    assert!(row("Snug").contains("Same as <code>Small</code>"), "{page}");
    assert!(!row("Medium").contains("Same as"), "{page}");
}

#[test]
fn a_row_of_words_in_bold_and_nothing_else_heads_a_group_rather_than_being_a_name() {
    let text = SPACES.replace("|---|---|---|\n", "|---|---|---|\n| **Small ones** | | |\n");

    let page = draw(&foundations(&text, &[]), &components()).into_string();

    assert!(page.contains("<tr class=\"group\"><th colspan=\"4\"><strong>Small ones</strong></th></tr>"), "{page}");
    assert!(!page.contains("id=\"Space-Small ones\""), "{page}");
}

#[test]
fn a_sections_rules_are_folded_away_only_where_it_has_names_to_look_up() {
    let page = draw(&foundations(SPACES, &[]), &components()).into_string();

    let (space, pointer) = page.split_once("<section id=\"foundations-the-pointer\">").unwrap();
    assert!(space.contains("<details class=\"rules-folded\"><summary>The rules, 1</summary>"), "{page}");
    assert!(!pointer.contains("<details"), "{page}");
    assert!(pointer.contains("A hand over what can be pressed."), "{page}");
    assert!(page.contains("<details class=\"sheet\"><summary>The whole sheet</summary>"), "{page}");
}

#[test]
fn the_headings_are_listed_each_under_the_one_above_with_how_many_names_each_has() {
    let text = "# Look\n\n## Space and shape\n\n### Space\n\n**Every step, by its name.** Each is in `look::Space`.\n\n\
                | Name | Room | For |\n|---|---|---|\n| `Small` | 8px | Gaps |\n\n### The window\n\n- **No title bar.**\n";

    let page = draw(&foundations(text, &[]), &components()).into_string();

    let nav = page.split("<nav>").nth(1).unwrap().split("</nav>").next().unwrap();
    assert_eq!(
        nav,
        "<ul><li class=\"half\"><a href=\"#foundations\">Foundations</a><ul><li><a href=\"#foundations-space-and-shape\">\
         Space and shape</a><ul><li><a href=\"#foundations-space\">Space <span class=\"count\">1</span></a></li><li>\
         <a href=\"#foundations-the-window\">The window</a></li></ul></li></ul></li><li class=\"half\">\
         <a href=\"#components\">Components</a><ul><li><a href=\"#components-buttons\">Buttons</a></li></ul></li></ul>"
    );
}

#[test]
fn the_components_follow_the_foundations_under_a_heading_of_their_own_with_their_rules_shown() {
    let page = draw(&foundations(SPACES, &[]), &components()).into_string();

    let (foundations, components) = page.split_once("<section class=\"half\" id=\"components\">").unwrap();
    assert!(foundations.contains("<section class=\"half\" id=\"foundations\"><h2>Foundations</h2>"), "{page}");
    assert!(foundations.contains("<section id=\"foundations-space\"><h3>Space</h3>"), "{page}");
    assert!(components.starts_with("<h2>Components</h2><div class=\"intro\"><p>The pieces screens"), "{page}");
    assert!(components.contains("<section id=\"components-buttons\"><h3>Buttons</h3><div class=\"rules\">"), "{page}");
    assert!(components.contains("One main action an area."), "{page}");
}

#[test]
fn the_page_is_in_the_colours_the_spec_names() {
    let text = "# Look\n\n**Every colour, by its name.** Each is a field of `look::Colors`.\n\n\
                | Name | Colour | For |\n|---|---|---|\n| **Surfaces** | | |\n| `pane` | `#151312` | The pane |\n\
                | `not_one` | Copper | Isn't a colour's value |\n";

    let page = draw(&foundations(text, &[]), &components()).into_string();

    assert!(page.contains(":root {\n  --pane: #151312;\n}"), "{page}");
}

/// The spec's file as it is, and its sheets' places, which the gallery keeps.
fn as_it_is(file: &str) -> Area {
    let spec = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/spec");
    let mut index = read(&spec).unwrap();
    let at = index.areas.iter().position(|area| area.file == file).unwrap();
    index.areas.swap_remove(at)
}

#[test]
fn every_name_in_the_spec_has_a_piece_of_its_sheet_and_every_piece_is_a_name() {
    let mut problems = Vec::new();
    let mut shown = Vec::new();

    for area in [as_it_is(FOUNDATIONS), as_it_is(COMPONENTS)] {
        check_names(&area, &mut problems, &mut shown);
    }

    assert!(problems.is_empty(), "{problems:#?}");
    assert!(shown.len() >= 8, "the spec's tables of names: {shown:?}");
}

/// Each name in `area`'s tables against its piece of its sheet, and each piece of a sheet in `area` against a name.
fn check_names(area: &Area, problems: &mut Vec<String>, shown: &mut Vec<String>) {
    let start = shown.len();
    for (_, named) in all_named(area) {
        let Some(sheet) = named.sheet else {
            problems.push(format!("no sheet has places for {}", named.kind));
            continue;
        };
        let places = &sheet.places[named.kind];
        let names: Vec<String> = named.rows().map(|row| text(&row[0])).collect();
        let unplaced = names.iter().filter(|name| !places.contains_key(*name));
        problems.extend(unplaced.map(|name| format!("{}'s sheet has no place for {name}", named.kind)));
        let unnamed = places.keys().filter(|name| !names.contains(name));
        problems.extend(unnamed.map(|name| format!("{}'s table has no {name}, which its sheet has", named.kind)));
        shown.push(named.kind.to_string());
    }
    let here = &shown[start..];
    let kinds = area.screens().flat_map(|screen| screen.places.keys());
    let unnamed = kinds.filter(|kind| !here.contains(kind));
    problems.extend(unnamed.map(|kind| format!("no table in {} names {kind}", area.file)));
}

#[test]
fn every_colour_the_page_is_drawn_in_is_one_the_spec_names() {
    let area = as_it_is(FOUNDATIONS);
    let defined = colours(&area);

    let used = STYLE.split("var(--").skip(1).filter_map(|after| after.split(')').next());
    let undefined: Vec<&str> = used.filter(|name| !defined.contains(&format!("--{name}: #"))).collect();

    assert!(undefined.is_empty(), "the page uses colours {COLOURS} doesn't name: {undefined:?}");
}

#[test]
fn the_page_is_written_beside_the_specs_and_only_from_the_foundations_and_the_components() {
    let out = tempfile::tempdir().unwrap();
    let menus = || area("menus.md", "Menus", "# Menus\n").unwrap();

    let no_foundations = write(&Index { areas: vec![menus(), components()] }, out.path()).expect_err("refused");
    let no_components =
        write(&Index { areas: vec![menus(), foundations(SPACES, &[])] }, out.path()).expect_err("refused");
    let page = write(&Index { areas: vec![foundations(SPACES, &[]), menus(), components()] }, out.path()).unwrap();

    assert_eq!(no_foundations.to_string(), "the spec has no look.md");
    assert_eq!(no_components.to_string(), "the spec has no components.md");
    assert_eq!(page.file_name().unwrap(), "design.html");
    assert!(std::fs::read_to_string(page).unwrap().contains("<title>The design system</title>"));
}
