use std::collections::HashSet;

use super::*;

const PATHS: [&str; 6] = [
    "README.md",
    "apps/desktop/src/app.rs",
    "apps/desktop/src/app/view.rs",
    "apps/desktop/src/lib.rs",
    "apps/desktop/tests/ui/main.rs",
    "docs/spec/diff.md",
];

fn folder(path: &str, names: &[&str], depth: usize, open: bool) -> Row {
    let names = names.iter().map(|name| (*name).to_string()).collect();
    Row::Folder { path: path.to_string(), names, depth, open }
}

fn file(index: usize, depth: usize) -> Row {
    Row::File { index, depth }
}

#[test]
fn folders_come_first_and_a_folder_that_holds_only_a_folder_shares_its_row() {
    let rows = Tree::new(PATHS).rows(|_| true);

    assert_eq!(
        rows,
        vec![
            folder("apps/desktop", &["apps", "desktop"], 0, true),
            folder("apps/desktop/src", &["src"], 1, true),
            folder("apps/desktop/src/app", &["app"], 2, true),
            file(2, 3),
            file(1, 2),
            file(3, 2),
            folder("apps/desktop/tests/ui", &["tests", "ui"], 1, true),
            file(4, 2),
            folder("docs/spec", &["docs", "spec"], 0, true),
            file(5, 1),
            file(0, 0),
        ]
    );
}

#[test]
fn a_closed_folder_hides_what_it_holds() {
    let closed = HashSet::from(["apps/desktop/src".to_string()]);

    let rows = Tree::new(PATHS).rows(|path| !closed.contains(path));

    assert_eq!(rows[1], folder("apps/desktop/src", &["src"], 1, false));
    assert_eq!(rows[2], folder("apps/desktop/tests/ui", &["tests", "ui"], 1, true));
}

/// All files starts with every folder closed, and opens them one at a time.
#[test]
fn only_the_folders_opened_show_what_they_hold() {
    // Opening a row opens each folder it stands for.
    let open = HashSet::from(["apps".to_string(), "apps/desktop".to_string()]);

    let rows = Tree::new(PATHS).rows(|path| open.contains(path));

    assert_eq!(
        rows,
        vec![
            folder("apps/desktop", &["apps", "desktop"], 0, true),
            folder("apps/desktop/src", &["src"], 1, false),
            folder("apps/desktop/tests/ui", &["tests", "ui"], 1, false),
            folder("docs/spec", &["docs", "spec"], 0, false),
            file(0, 0),
        ]
    );
}

/// Folders that had rows of their own can disagree once they share one: closing `docs` while it held a file left
/// `docs/spec` open, and deleting the file joins them. The row shows what was done last to the folder it starts with.
#[test]
fn a_row_of_folders_is_open_only_while_each_of_them_is() {
    let open = HashSet::from(["docs/spec".to_string()]);
    let closed = HashSet::from(["docs".to_string()]);

    let all_files = Tree::new(PATHS).rows(|path| open.contains(path));
    let changes = Tree::new(PATHS).rows(|path| !closed.contains(path));

    assert_eq!(all_files[1], folder("docs/spec", &["docs", "spec"], 0, false), "All files, which keeps the open ones");
    assert_eq!(changes[8], folder("docs/spec", &["docs", "spec"], 0, false), "Changes, which keeps the closed ones");
    assert_eq!(changes[9], file(0, 0), "and what's in it doesn't show");
}

/// `.context` holds only `gallery`, and opening it reads that much, so sharing a row with it would look as though
/// opening it had renamed it.
#[test]
fn a_folder_read_when_it_s_opened_keeps_a_row_of_its_own() {
    let open = HashSet::from([".context".to_string()]);

    let rows = Tree::with_folders(["README.md"], [".context", ".context/gallery", "vendor/inner"])
        .rows(|path| open.contains(path));

    assert_eq!(
        rows,
        vec![
            folder(".context", &[".context"], 0, true),
            folder(".context/gallery", &["gallery"], 1, false),
            folder("vendor/inner", &["vendor", "inner"], 0, false),
            file(0, 0),
        ],
        "a folder above one can still share its row"
    );
}

#[test]
fn a_path_splits_into_its_folders_and_its_name() {
    assert_eq!(split("apps/desktop/src/app.rs"), ("apps/desktop/src/", "app.rs"));
    assert_eq!(split("README.md"), ("", "README.md"));
}

#[test]
fn a_row_of_folders_stands_for_each_of_them() {
    let names = |names: &[&str]| names.iter().map(ToString::to_string).collect::<Vec<_>>();

    assert_eq!(chain("apps/desktop", &names(&["apps", "desktop"])), ["apps", "apps/desktop"]);
    assert_eq!(chain("crates/hub/src", &names(&["hub", "src"])), ["crates/hub", "crates/hub/src"]);
    assert_eq!(chain("docs", &names(&["docs"])), ["docs"]);
}
