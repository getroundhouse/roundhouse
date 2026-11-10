//! Paths in their folders, as the sidebar of changes and All files show them: which rows they make, and how a row is
//! drawn. Each list keeps its rows, and lays them out again only when its files or its open folders change, since a big
//! repository has thousands.

use std::collections::BTreeMap;

mod row;
#[cfg(test)]
mod tests;

pub(crate) use row::{List, file_frame, folder_row, row_frame};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Row {
    /// A folder, or a chain of folders that hold nothing but the next, which shows as one.
    Folder {
        /// The whole path to the last folder in the chain, which is what opening or closing it remembers.
        path: String,
        /// Each folder in the chain, outermost first.
        names: Vec<String>,
        depth: usize,
        open: bool,
    },
    /// A file, by where its path is in the paths the tree was made from.
    File { index: usize, depth: usize },
}

/// The folders some paths are in.
#[derive(Debug, Default)]
pub(crate) struct Tree {
    root: Folder,
}

#[derive(Debug, Default)]
struct Folder {
    folders: BTreeMap<String, Folder>,
    files: BTreeMap<String, usize>,
    /// It's read only when it's opened, so it never shares a row with a folder inside it, which would make opening it
    /// look like renaming it.
    own_row: bool,
}

impl Tree {
    pub(crate) fn new<'a>(paths: impl IntoIterator<Item = &'a str>) -> Self {
        Self::with_folders(paths, [])
    }

    /// The folders `paths` are in, and `folders` too, which are folders whatever they hold, since what's in each is
    /// read only when it's opened.
    pub(crate) fn with_folders<'a>(
        paths: impl IntoIterator<Item = &'a str>,
        folders: impl IntoIterator<Item = &'a str>,
    ) -> Self {
        let mut root = Folder::default();
        for path in folders {
            root.folder(path.split('/')).own_row = true;
        }
        for (index, path) in paths.into_iter().enumerate() {
            let (folders, name) = path.rsplit_once('/').map_or((None, path), |(dir, name)| (Some(dir), name));
            root.folder(folders.into_iter().flat_map(|dir| dir.split('/'))).files.insert(name.to_string(), index);
        }
        Self { root }
    }

    /// The rows that show: in each folder, the folders first and then the files, each by name, as file browsers show
    /// them. A folder shows what's in it when `open` says so of its path, and of each folder its row stands for.
    pub(crate) fn rows(&self, open: impl Fn(&str) -> bool) -> Vec<Row> {
        let mut out = Vec::new();
        self.root.push_rows("", 0, &open, &mut out);
        out
    }
}

impl Folder {
    /// The folder at the end of `parts`, made if it isn't there.
    fn folder<'a>(&mut self, parts: impl IntoIterator<Item = &'a str>) -> &mut Folder {
        let mut folder = self;
        for part in parts {
            folder = folder.folders.entry(part.to_string()).or_default();
        }
        folder
    }

    fn push_rows(&self, path: &str, depth: usize, open: &dyn Fn(&str) -> bool, out: &mut Vec<Row>) {
        for (name, folder) in &self.folders {
            let (mut names, mut inner) = (vec![name.clone()], folder);
            while let Some((next, deeper)) = inner.only_folder() {
                names.push(next.clone());
                inner = deeper;
            }
            // A row is open only if every folder it stands for is, since they can disagree: closing `docs` while it
            // held a file too left `docs/spec` open, and deleting that file joins them in one row.
            let mut full = path.to_string();
            let mut is_open = true;
            for name in &names {
                if !full.is_empty() {
                    full.push('/');
                }
                full.push_str(name);
                is_open &= open(&full);
            }
            out.push(Row::Folder { path: full.clone(), names, depth, open: is_open });
            if is_open {
                inner.push_rows(&full, depth + 1, open, out);
            }
        }
        out.extend(self.files.values().map(|index| Row::File { index: *index, depth }));
    }

    /// The one folder inside this one, when it holds nothing else, and can share its row.
    fn only_folder(&self) -> Option<(&String, &Folder)> {
        let mut folders = self.folders.iter();
        match (folders.next(), folders.next()) {
            (Some(only), None) if self.files.is_empty() && !self.own_row => Some(only),
            _ => None,
        }
    }
}

/// The path of each folder a folder's row stands for, outermost first, from the row's `path` and `names`: `apps` and
/// `apps/desktop` for `apps / desktop`. Opening or closing the row opens or closes them all, so the row stays open or
/// closed if a new file splits it in two.
pub(crate) fn chain(path: &str, names: &[String]) -> Vec<String> {
    let base = path.strip_suffix(names.join("/").as_str()).unwrap_or_default();
    let mut folder = base.to_string();
    names
        .iter()
        .map(|name| {
            folder.push_str(name);
            let path = folder.clone();
            folder.push('/');
            path
        })
        .collect()
}

/// A path's folders, with the slash after them, and its file's name.
pub(crate) fn split(path: &str) -> (&str, &str) {
    path.rfind('/').map_or(("", path), |at| (&path[..=at], &path[at + 1..]))
}
