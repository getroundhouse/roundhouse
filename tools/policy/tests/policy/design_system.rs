//! The design system's boundary: screens draw with its components rather than with the kit's own parts that the
//! components hold, as `docs/component-library.md` says.

use std::collections::BTreeSet;
use std::path::Path;

use super::rust_standards::{add_rust_files, covered_crates};
use super::workspace::workspace;

/// One of the kit's parts that a component holds: what names it in code, such as its module or the call that makes it,
/// the component's maker, which the failure says to use, and where it's used other than through the component, as
/// (path from the repo root, why). A path that ends in `/` is a folder, and covers everything in it.
struct KitPart {
    named: &'static str,
    instead: &'static str,
    allowed: &'static [(&'static str, &'static str)],
}

/// The kit's button, which `look::Button` holds.
const KIT_BUTTON: KitPart =
    KitPart { named: "component::button", instead: "look::button", allowed: KIT_BUTTON_ALLOWED };

/// The kit's key, which `look::Key` holds.
const KIT_KEY: KitPart = KitPart {
    named: "component::kbd",
    instead: "look::key",
    allowed: &[("crates/look/src/keys/", "the component library's key, which writes a key as the kit does")],
};

/// The kit's single-line input, which `look::Field` holds, named by the call that makes it, since what it holds, the
/// kit's `InputState`, is made where it's kept, such as by the composer, whose text area is the kit's own.
const KIT_INPUT: KitPart = KitPart {
    named: "Input::new(",
    instead: "look::field",
    allowed: &[
        ("crates/look/src/fields/", "the component library's field, which holds the kit's input"),
        (
            "crates/transcript/src/questions/panel/options.rs",
            "a question's typed answer, masked, set as its options' text, which the questions' component will draw",
        ),
    ],
};

/// The kit's switch, which `look::Toggle` holds.
const KIT_SWITCH: KitPart = KitPart {
    named: "component::switch",
    instead: "look::toggle",
    allowed: &[("crates/look/src/toggles/", "the component library's toggle, which holds the kit's switch")],
};

/// The kit's radio, which nothing draws, since choosing one of a few is a `look::Switch`.
const KIT_RADIO: KitPart = KitPart { named: "component::radio", instead: "look::switch", allowed: &[] };

/// The kit's menu, which `look::Menu` fills, and which opens from a `look::Button` or a right-click through `look`.
const KIT_MENU: KitPart = KitPart {
    named: "component::menu",
    instead: "look::menu_row",
    allowed: &[
        ("crates/look/src/menus/", "the component library's menu, which fills the kit's and sets how every one looks"),
        ("crates/look/src/controls/", "the component library's button, which opens a menu with the kit's"),
        ("apps/desktop/src/menus/", "the app's menus off the Mac, each of the kit's own, with its submenus and keys"),
        ("apps/desktop/src/app/menu_button.rs", "the menu bar off the Mac, whose buttons hold their menus open"),
        (
            "apps/desktop/src/sidebar/hub.rs",
            "the hub switcher, the kit's button drawn as a field, which opens a look::Menu with the kit's",
        ),
        (
            "apps/desktop/src/workspace_pane/closed.rs",
            "the closed chats' button, the kit's, lit by its icon, which opens a look::Menu with the kit's",
        ),
    ],
};

const KIT_BUTTON_ALLOWED: &[(&str, &str)] = &[
    ("crates/look/src/controls/", "the component library's button, which holds the kit's"),
    (
        "crates/look/src/hand.rs",
        "its tests, of the hand and greying out on the kit's own button, which keeps the arrow",
    ),
    ("apps/desktop/src/sidebar/row.rs", "a workspace's row, drawn as a button, which the rows' component will draw"),
    ("apps/desktop/src/sidebar/nav.rs", "the sidebar's rows that go somewhere, drawn as buttons, as a workspace's are"),
    (
        "apps/desktop/src/sidebar/hub.rs",
        "the hub switcher, a choice drawn as a field, which the fields' component will draw",
    ),
    ("apps/desktop/src/workspace_pane/closed.rs", "closed chats, lit by its icon rather than a fill under the pointer"),
    (
        "crates/transcript/src/list/transcript_list.rs",
        "the button back to a chat's end, edged over the chat's text, which keeps its look as it fades out",
    ),
];

/// Whether `line` names `path` in its code, rather than in a string, such as a made-up file a test shows.
fn names_in_code(line: &str, path: &str) -> bool {
    line.match_indices(path).any(|(at, _)| line[..at].matches('"').count().is_multiple_of(2))
}

/// Every file of the crates the checks cover that names `part` in its code, by its path from the repo root.
fn using(part: &KitPart) -> BTreeSet<String> {
    let metadata = workspace();
    let root = Path::new(metadata["workspace_root"].as_str().unwrap());
    let mut files = Vec::new();
    for dir in covered_crates(&metadata) {
        add_rust_files(&dir, &mut files);
    }
    files
        .iter()
        .filter(|file| std::fs::read_to_string(file).unwrap().lines().any(|line| names_in_code(line, part.named)))
        .map(|file| file.strip_prefix(root).unwrap().display().to_string())
        .collect()
}

/// That `part` is used only through its component, but in the places its list names, and that each of those still uses
/// it.
fn only_the_component_library_uses(part: &KitPart) {
    let using = using(part);
    // A part nothing may use but its component is found there, so finding none means the search went wrong.
    assert!(
        part.allowed.is_empty() || !using.is_empty(),
        "found nothing using {}, so this isn't looking in the right place",
        part.named
    );
    let allowed = |path: &str, (place, _): &(&str, &str)| match place.strip_suffix('/') {
        Some(folder) => path.starts_with(&format!("{folder}/")),
        None => path == *place,
    };
    let unexplained: Vec<_> =
        using.iter().filter(|path| !part.allowed.iter().any(|entry| allowed(path, entry))).collect();
    assert!(unexplained.is_empty(), "these use the kit's {}, so use `{}`: {unexplained:?}", part.named, part.instead);
    let stale: Vec<_> = part.allowed.iter().filter(|entry| !using.iter().any(|path| allowed(path, entry))).collect();
    assert!(stale.is_empty(), "these don't use the kit's {} now, so take them off its list: {stale:?}", part.named);
}

#[test]
fn only_the_component_library_uses_the_kits_button() {
    assert!(using(&KIT_BUTTON).len() > 2, "found too few, so this isn't looking in the right place");
    only_the_component_library_uses(&KIT_BUTTON);
}

#[test]
fn only_the_component_library_uses_the_kits_key() {
    only_the_component_library_uses(&KIT_KEY);
}

#[test]
fn only_the_component_library_uses_the_kits_input() {
    only_the_component_library_uses(&KIT_INPUT);
}

#[test]
fn only_the_component_library_uses_the_kits_switch() {
    only_the_component_library_uses(&KIT_SWITCH);
}

#[test]
fn only_the_component_library_fills_the_kits_menu() {
    only_the_component_library_uses(&KIT_MENU);
}

#[test]
fn nothing_uses_the_kits_radio() {
    only_the_component_library_uses(&KIT_RADIO);
}

#[test]
fn a_path_in_a_string_isnt_a_use() {
    // Made from the path, so this file doesn't name it in code.
    let used = format!("use gpui_kit::{}::Button;", KIT_BUTTON.named);
    assert!(names_in_code(&used, KIT_BUTTON.named));
    assert!(!names_in_code(&format!("tool_result(\"1\", \"{used}\")"), KIT_BUTTON.named));
}
