use std::rc::Rc;

use gpui_kit::OperatingSystem;
use gpui_kit::assets::IconName as Lucide;

use super::sidebar::find_key_says;
use super::{Control, Page, PageId, Section, Setting, found, shown};

fn setting(name: &'static str, does: &'static str) -> Setting {
    Setting { name, does, control: Control::Switch { is_on: Box::new(|_| Some(false)), set: Rc::new(|_, _| {}) } }
}

/// A page made up for a test. Only `shown` tells pages apart by their ID, and there's one page to tell apart.
fn page(name: &'static str, settings: Vec<Setting>) -> Page {
    Page { id: PageId::General, name, icon: Lucide::Settings2, settings }
}

fn all() -> Vec<Section> {
    let general = page("General", vec![setting("Dark", "Darker colours."), setting("Awake", "Never sleep.")]);
    let sounds = page("Sounds", vec![setting("Chime", "A sound when an agent is done.")]);
    let git = page("Git", vec![setting("Prune", "Remove old branches.")]);
    vec![
        Section { heading: "Personal", pages: vec![general, sounds] },
        Section { heading: "Project", pages: vec![git] },
    ]
}

/// What's left, as an outline: each heading, then each of its pages, then each of their settings.
fn left(sections: &[Section]) -> Vec<String> {
    let mut lines = Vec::new();
    for section in sections {
        lines.push(section.heading.to_string());
        for page in &section.pages {
            lines.push(format!("{} / {}", section.heading, page.name));
            let settings = page.settings.iter();
            lines.extend(settings.map(|setting| format!("{} / {} / {}", section.heading, page.name, setting.name)));
        }
    }
    lines
}

#[test]
fn nothing_typed_leaves_everything() {
    assert_eq!(left(&found(all(), "")), left(&all()));
    assert_eq!(left(&found(all(), "  ")), left(&all()));
}

#[test]
fn a_search_leaves_what_matches_by_name_or_by_what_it_does_in_any_case() {
    assert_eq!(left(&found(all(), "dark")), ["Personal", "Personal / General", "Personal / General / Dark"]);
    assert_eq!(left(&found(all(), "AGENT ")), ["Personal", "Personal / Sounds", "Personal / Sounds / Chime"]);
}

#[test]
fn a_page_or_heading_with_nothing_left_goes() {
    assert_eq!(left(&found(all(), "branches")), ["Project", "Project / Git", "Project / Git / Prune"]);
    assert!(found(all(), "nothing like it").is_empty());
}

/// The real pages, under their headings, as nobody has searched.
fn real() -> Vec<Section> {
    super::sections(None)
}

#[test]
fn the_page_chosen_shows() {
    let sections = real();

    let page = shown(&sections, Some(PageId::General));

    assert_eq!(page.map(|page| page.id), Some(PageId::General));
}

#[test]
fn with_nothing_chosen_the_first_page_shows() {
    let sections = real();

    let page = shown(&sections, None);

    assert_eq!(page.map(|page| page.id), Some(sections[0].pages[0].id));
}

#[test]
fn with_nothing_left_by_the_search_no_page_shows() {
    let sections = found(real(), "nothing like it");

    assert!(shown(&sections, Some(PageId::General)).is_none());
}

/// The search shows the key that finds a setting: Cmd on a Mac and Ctrl elsewhere, as it's bound.
#[test]
fn the_search_shows_its_key_as_this_system_writes_it() {
    let (os, says) = if cfg!(target_os = "macos") {
        (OperatingSystem::Mac, vec!["⌘", "F"])
    } else {
        (OperatingSystem::Linux, vec!["Ctrl+F"])
    };

    assert_eq!(find_key_says(os), says);
}
