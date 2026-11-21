use std::rc::Rc;

use client_core::records::Snapshot;
use gpui_kit::assets::IconName as Lucide;

use super::{Body, Control, OfHub, Page, PageId, Section, Setting, found, shown};

fn setting(name: &'static str, does: &'static str) -> Setting {
    let control = Control::Switch { is_on: Box::new(|_| Some(false)), set: Rc::new(|_, _| {}) };
    Setting { name, does, control, status_box: None }
}

/// A page made up for a test. Only `shown` tells pages apart by their ID, and there's one page to tell apart.
fn page(name: &'static str, settings: Vec<Setting>) -> Page {
    Page { id: PageId::General, name, icon: Lucide::Settings2, body: Body::Settings(settings) }
}

/// A page that draws what's on it itself, about `about`.
fn own(name: &'static str, about: &'static str) -> Page {
    let draw = Box::new(|_: &_| unreachable!("a search doesn't draw a page"));
    Page { id: PageId::Agents, name, icon: Lucide::Bot, body: Body::Own { about, draw } }
}

fn all() -> Vec<Section> {
    let general = page("General", vec![setting("Dark", "Darker colours."), setting("Awake", "Never sleep.")]);
    let sounds = page("Sounds", vec![setting("Chime", "A sound when an agent is done.")]);
    let git = page("Git", vec![setting("Prune", "Remove old branches.")]);
    let keys = own("Keys", "The logins agents run under, from a token.");
    vec![
        Section { heading: "Personal", pages: vec![general, sounds] },
        Section { heading: "Project", pages: vec![git, keys] },
    ]
}

/// What's left, as an outline: each heading, then each of its pages, then each of their settings. A page of its own
/// is left whole, and lists nothing under it.
fn left(sections: &[Section]) -> Vec<String> {
    let mut lines = Vec::new();
    for section in sections {
        lines.push(section.heading.to_string());
        for page in &section.pages {
            lines.push(format!("{} / {}", section.heading, page.name));
            let Body::Settings(settings) = &page.body else { continue };
            lines.extend(
                settings.iter().map(|setting| format!("{} / {} / {}", section.heading, page.name, setting.name)),
            );
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
    assert_eq!(
        left(&found(all(), "AGENT ")),
        ["Personal", "Personal / Sounds", "Personal / Sounds / Chime", "Project", "Project / Keys"]
    );
}

#[test]
fn a_page_of_its_own_is_found_whole_by_its_name_or_what_its_about() {
    assert_eq!(left(&found(all(), "keys")), ["Project", "Project / Keys"]);
    assert_eq!(left(&found(all(), "Token")), ["Project", "Project / Keys"]);
    assert_eq!(
        left(&found(all(), "prune")),
        ["Project", "Project / Git", "Project / Git / Prune"],
        "and not by another's"
    );
}

#[test]
fn a_page_or_heading_with_nothing_left_goes() {
    assert_eq!(left(&found(all(), "branches")), ["Project", "Project / Git", "Project / Git / Prune"]);
    assert!(found(all(), "nothing like it").is_empty());
}

/// The real pages, under their headings, as nobody has searched, with no hub yet.
fn real() -> Vec<Section> {
    super::sections(&OfHub { records: Snapshot::default(), asking: None })
}

#[test]
fn agents_is_found_by_what_someone_would_look_for_it_by() {
    for search in ["agents", "accounts", "login", "token", "Connect", "renew", "disconnect"] {
        let pages: Vec<PageId> =
            found(real(), search).iter().flat_map(|section| &section.pages).map(|page| page.id).collect();

        assert!(pages.contains(&PageId::Agents), "{search} doesn't find it, only {pages:?}");
    }
    let pages = found(real(), "sound");
    assert!(pages.iter().flat_map(|section| &section.pages).all(|page| page.id != PageId::Agents));
}

#[test]
fn the_page_chosen_shows_or_the_first_with_none_chosen_or_none_with_nothing_left_by_the_search() {
    let first = real()[0].pages[0].id;
    let cases = [
        ("the page chosen", real(), Some(PageId::Agents), Some(PageId::Agents)),
        ("nothing chosen", real(), None, Some(first)),
        ("nothing left by the search", found(real(), "nothing like it"), Some(PageId::General), None),
    ];

    for (name, sections, chosen, want) in cases {
        assert_eq!(shown(&sections, chosen).map(|page| page.id), want, "{name}");
    }
}

/// What each real page lists, by name.
fn names_on(page: PageId) -> Vec<&'static str> {
    let sections = real();
    let page = sections.iter().flat_map(|section| &section.pages).find(|shown| shown.id == page);
    match page.map(|page| &page.body) {
        Some(Body::Settings(settings)) => settings.iter().map(|setting| setting.name).collect(),
        Some(Body::Own { .. }) | None => Vec::new(),
    }
}

/// Dictation and speech are on Voice, right after General, where a device can hear, and General has none of them. A
/// device that can't hear has no Voice page.
#[test]
fn dictation_and_speech_are_on_voice_after_general() {
    let pages: Vec<PageId> = real().iter().flat_map(|section| &section.pages).map(|page| page.id).collect();
    if !sound::HEARS {
        assert!(!pages.contains(&PageId::Voice), "{pages:?}");
        return;
    }
    assert_eq!(pages[..2], [PageId::General, PageId::Voice]);
    let voice = names_on(PageId::Voice);
    assert_eq!(voice.first(), Some(&"Dictation"));
    assert_eq!(voice[voice.len() - 2..], ["Speech", "Speech speed"]);
    assert!(voice.contains(&"Dictation language"), "{voice:?}");
    let general = names_on(PageId::General);
    assert!(general.iter().all(|name| !name.contains("Dictation") && !name.contains("Speech")), "{general:?}");
}
