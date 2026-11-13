//! Which rows the changed files make, as a list of paths or in folders.

use std::collections::HashSet;

use crate::file_tree::{Row, Tree};

/// How the changed files are laid out. The same in every workspace, so it's chosen once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Layout {
    /// Each file's whole path, in order.
    #[default]
    List,
    /// Files in their folders, which open and close.
    Tree,
}

/// The rows for `paths`, which are in order. A folder in `closed` shows without what's in it, and the rest are open.
pub(crate) fn rows<'a>(layout: Layout, paths: impl IntoIterator<Item = &'a str>, closed: &HashSet<String>) -> Vec<Row> {
    match layout {
        Layout::List => paths.into_iter().enumerate().map(|(index, _)| Row::File { index, depth: 0 }).collect(),
        Layout::Tree => Tree::new(paths).rows(|path| !closed.contains(path)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATHS: [&str; 3] = ["README.md", "apps/desktop/src/app.rs", "docs/spec/diff.md"];

    #[test]
    fn a_list_is_every_path_in_order() {
        let rows = rows(Layout::List, PATHS, &HashSet::new());

        assert_eq!(rows, Vec::from_fn(PATHS.len(), |index| Row::File { index, depth: 0 }));
    }

    #[test]
    fn in_folders_every_folder_starts_open() {
        let rows = rows(Layout::Tree, PATHS, &HashSet::new());

        assert!(rows.iter().all(|row| !matches!(row, Row::Folder { open: false, .. })), "{rows:?}");
        assert_eq!(rows.len(), 5, "two folders and three files: {rows:?}");
    }
}
