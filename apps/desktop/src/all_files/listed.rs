//! The worktree's files as All files lays them out: what the hub listed of the worktree, with what's in each folder
//! read since, in their folders.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use gpui_kit::SharedString;
use protocol::model::WorktreeFiles;

use crate::file_tree::Tree;

pub(super) struct Listed {
    /// Every file known, which the tree's rows name by where they are in it.
    pub(super) paths: Vec<SharedString>,
    /// Folders whose files are read only when they're opened, by path.
    pub(super) folders: HashSet<String>,
    pub(super) tree: Tree,
    /// The hub left some out, since there were so many.
    pub(super) truncated: bool,
}

impl Listed {
    /// Lays out `worktree`, and each folder in `read` that's still a folder to read, which takes a moment for a big
    /// repository, so it's for a background thread. A folder read before that's no longer listed as one, perhaps since
    /// it's no longer ignored, is left out, since its files are listed with the worktree's.
    pub(super) fn new(worktree: &WorktreeFiles, read: &BTreeMap<String, Arc<WorktreeFiles>>) -> Self {
        let mut folders: HashSet<String> = worktree.folders.iter().cloned().collect();
        let mut listings = vec![worktree];
        // In order of path, a folder comes before the folders inside it.
        for (path, listing) in read {
            if folders.contains(path) {
                folders.extend(listing.folders.iter().cloned());
                listings.push(listing.as_ref());
            }
        }
        let paths: Vec<SharedString> =
            listings.iter().flat_map(|listing| listing.paths.iter()).map(|path| path.clone().into()).collect();
        let tree = Tree::with_folders(paths.iter().map(AsRef::as_ref), folders.iter().map(String::as_str));
        let truncated = listings.iter().any(|listing| listing.truncated);
        Self { paths, folders, tree, truncated }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(paths: &[&str], folders: &[&str]) -> Arc<WorktreeFiles> {
        let strings = |names: &[&str]| names.iter().map(ToString::to_string).collect();
        Arc::new(WorktreeFiles { paths: strings(paths), folders: strings(folders), truncated: false })
    }

    #[test]
    fn a_folder_read_adds_its_files_and_its_folders_and_one_no_longer_listed_doesn_t() {
        let worktree = files(&["a.txt"], &["target"]);
        let read = BTreeMap::from([
            ("target".to_string(), files(&["target/CACHEDIR.TAG"], &["target/debug"])),
            ("target/debug".to_string(), files(&["target/debug/app"], &[])),
            ("build".to_string(), files(&["build/old.txt"], &[])),
        ]);

        let listed = Listed::new(&worktree, &read);

        let paths: Vec<&str> = listed.paths.iter().map(AsRef::as_ref).collect();
        assert_eq!(paths, ["a.txt", "target/CACHEDIR.TAG", "target/debug/app"]);
        assert_eq!(listed.folders, HashSet::from(["target".to_string(), "target/debug".to_string()]));
    }
}
