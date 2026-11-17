//! The design system's boundary: screens draw with its components rather than with the kit's own parts that the
//! components hold, as `docs/component-library.md` says.

use std::collections::BTreeSet;
use std::path::Path;

use super::rust_standards::{add_rust_files, covered_crates};
use super::workspace::workspace;

/// The kit's button, which `look::Button` holds, named in code as its module is.
const KIT_BUTTON: &str = "component::button";

/// Where the kit's button is used other than through `look::Button`, as (path from the repo root, why). A path that
/// ends in `/` is a folder, and covers everything in it.
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

#[test]
fn only_the_component_library_uses_the_kits_button() {
    let metadata = workspace();
    let root = Path::new(metadata["workspace_root"].as_str().unwrap());
    let mut files = Vec::new();
    for dir in covered_crates(&metadata) {
        add_rust_files(&dir, &mut files);
    }
    let using: BTreeSet<String> = files
        .iter()
        .filter(|file| std::fs::read_to_string(file).unwrap().lines().any(|line| names_in_code(line, KIT_BUTTON)))
        .map(|file| file.strip_prefix(root).unwrap().display().to_string())
        .collect();
    assert!(using.len() > 2, "found only {using:?}, so this isn't looking in the right place");
    let allowed = |path: &str, (place, _): &(&str, &str)| match place.strip_suffix('/') {
        Some(folder) => path.starts_with(&format!("{folder}/")),
        None => path == *place,
    };
    let unexplained: Vec<_> =
        using.iter().filter(|path| !KIT_BUTTON_ALLOWED.iter().any(|entry| allowed(path, entry))).collect();
    assert!(unexplained.is_empty(), "these use the kit's button, so make theirs with `look::button`: {unexplained:?}");
    let stale: Vec<_> =
        KIT_BUTTON_ALLOWED.iter().filter(|entry| !using.iter().any(|path| allowed(path, entry))).collect();
    assert!(stale.is_empty(), "these don't use the kit's button now, so take them off `KIT_BUTTON_ALLOWED`: {stale:?}");
}

#[test]
fn a_path_in_a_string_isnt_a_use() {
    // Made from the path, so this file doesn't name it in code.
    let used = format!("use gpui_kit::{KIT_BUTTON}::Button;");
    assert!(names_in_code(&used, KIT_BUTTON));
    assert!(!names_in_code(&format!("tool_result(\"1\", \"{used}\")"), KIT_BUTTON));
}
