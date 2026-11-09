//! Which rows the changed files make, as a list of paths or as folders.

use std::collections::{BTreeMap, HashSet};

/// How the changed files are laid out. The same in every workspace, so it's chosen once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Layout {
    /// Each file's whole path, in order.
    #[default]
    List,
    /// Files in their folders, which open and close.
    Tree,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Row {
    /// A folder, or a chain of folders that hold nothing but the next, which shows as one.
    Folder {
        /// The whole path to the last folder in the chain, which is what closing it remembers.
        path: String,
        /// Each folder in the chain, outermost first.
        names: Vec<String>,
        depth: usize,
        open: bool,
    },
    /// A changed file, by where it is in the paths given.
    File { index: usize, depth: usize },
}

/// The rows for `paths`, which are in order. A folder in `closed` shows without what's in it.
pub(crate) fn rows<'a>(layout: Layout, paths: impl IntoIterator<Item = &'a str>, closed: &HashSet<String>) -> Vec<Row> {
    match layout {
        Layout::List => paths.into_iter().enumerate().map(|(index, _)| Row::File { index, depth: 0 }).collect(),
        Layout::Tree => {
            let mut root = Folder::default();
            for (index, path) in paths.into_iter().enumerate() {
                let (folders, name) = path.rsplit_once('/').map_or((None, path), |(dir, name)| (Some(dir), name));
                let mut folder = &mut root;
                for part in folders.into_iter().flat_map(|dir| dir.split('/')) {
                    folder = folder.folders.entry(part).or_default();
                }
                folder.files.insert(name, index);
            }
            let mut out = Vec::new();
            root.push_rows("", 0, closed, &mut out);
            out
        }
    }
}

/// A path's folders, with the slash after them, and its file's name.
pub(crate) fn split(path: &str) -> (&str, &str) {
    path.rfind('/').map_or(("", path), |at| (&path[..=at], &path[at + 1..]))
}

#[derive(Default)]
struct Folder<'a> {
    folders: BTreeMap<&'a str, Folder<'a>>,
    files: BTreeMap<&'a str, usize>,
}

impl<'a> Folder<'a> {
    /// Folders come first, then files, each by name, as file browsers show them.
    fn push_rows(&self, path: &str, depth: usize, closed: &HashSet<String>, out: &mut Vec<Row>) {
        for (name, folder) in &self.folders {
            let (mut names, mut inner) = (vec![(*name).to_string()], folder);
            while let Some((next, deeper)) = inner.only_folder() {
                names.push(next.to_string());
                inner = deeper;
            }
            let joined = names.join("/");
            let full = if path.is_empty() { joined } else { format!("{path}/{joined}") };
            let open = !closed.contains(&full);
            out.push(Row::Folder { path: full.clone(), names, depth, open });
            if open {
                inner.push_rows(&full, depth + 1, closed, out);
            }
        }
        out.extend(self.files.values().map(|index| Row::File { index: *index, depth }));
    }

    /// The one folder inside this one, when it holds nothing else.
    fn only_folder(&self) -> Option<(&&'a str, &Folder<'a>)> {
        let mut folders = self.folders.iter();
        match (folders.next(), folders.next()) {
            (Some(only), None) if self.files.is_empty() => Some(only),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn a_list_is_every_path_in_order() {
        let rows = rows(Layout::List, PATHS, &HashSet::new());

        assert_eq!(rows, (0..PATHS.len()).map(|index| Row::File { index, depth: 0 }).collect::<Vec<_>>());
    }

    #[test]
    fn a_tree_puts_folders_first_and_joins_folders_that_hold_only_a_folder() {
        let rows = rows(Layout::Tree, PATHS, &HashSet::new());

        let file = |index, depth| Row::File { index, depth };
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

        let rows = rows(Layout::Tree, PATHS, &closed);

        assert_eq!(rows[1], folder("apps/desktop/src", &["src"], 1, false));
        assert_eq!(rows[2], folder("apps/desktop/tests/ui", &["tests", "ui"], 1, true));
    }

    #[test]
    fn a_path_splits_into_its_folders_and_its_name() {
        assert_eq!(split("apps/desktop/src/app.rs"), ("apps/desktop/src/", "app.rs"));
        assert_eq!(split("README.md"), ("", "README.md"));
    }
}
