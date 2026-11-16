//! What the copy was made from, and our patches on top of it, which [`crate::sync`] and [`crate::patch`] write beside
//! it, and read to bring the next release in or check the copy.

use std::fmt::{self, Write as _};
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::error::{At, Error};
use crate::files::{CopyPath, read_if_there};
use crate::patches::PatchName;
use crate::pin::{Commit, Kept, Release};

/// Where the record is, from the root of this repository.
pub const RECORD: &str = "third_party/gpui-kit/gpui-kit.toml";

/// A git tree's hash, for what [`crate::cut`] kept of upstream's release before any patch or notice was added.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct Tree(String);

impl Tree {
    pub fn new(hash: impl Into<String>) -> Self {
        Self(hash.into())
    }
}

impl fmt::Display for Tree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A merge onto a new release that stopped on a patch with conflicts left in the copy to settle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settling {
    /// Which patch in [`Record::patches`] stopped with conflicts.
    pub patch: PatchName,
    /// The commit the copy was moving from, whose blobs git merges later patches from.
    pub from: Commit,
    /// What the copy kept of `from`.
    pub from_kept: Vec<Kept>,
    /// The files `patch` left with conflicts.
    pub files: Vec<CopyPath>,
}

/// The release and commit of upstream's that the copy was made from, what it kept of them, and our patches on top.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub release: Release,
    pub commit: Commit,
    pub tree: Tree,
    pub kept: Vec<Kept>,
    pub patches: Vec<PatchName>,
    pub settling: Option<Settling>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Written {
    release: Release,
    commit: Commit,
    tree: Tree,
    kept: Vec<String>,
    #[serde(default)]
    patches: Vec<PatchName>,
    settling: Option<WrittenSettling>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WrittenSettling {
    patch: PatchName,
    from: Commit,
    from_kept: Vec<String>,
    files: Vec<CopyPath>,
}

impl Record {
    /// The record in the repository at `root`, or `None` when there isn't one.
    pub fn read(root: &Path) -> Result<Option<Self>, Error> {
        let path = root.join(RECORD);
        let Some(text) = read_if_there(&path)? else { return Ok(None) };
        let written: Written = toml::from_str(&text).map_err(|source| Error::Toml { path, source })?;
        let settling = written.settling.map(|settling| Settling {
            patch: settling.patch,
            from: settling.from,
            from_kept: settling.from_kept.iter().map(|path| Kept::from(path.as_str())).collect(),
            files: settling.files,
        });
        Ok(Some(Self {
            release: written.release,
            commit: written.commit,
            tree: written.tree,
            kept: written.kept.iter().map(|path| Kept::from(path.as_str())).collect(),
            patches: written.patches,
            settling,
        }))
    }

    pub(crate) fn write(&self, root: &Path) -> Result<(), Error> {
        let mut text = format!(
            "# The release of {} that {} was made from, what it kept of\n\
             # it, and our patches on top of it, which tools/kit-sync writes, and reads to bring the next release in.\n\
             release = {}\ncommit = {}\ntree = {}\nkept = {}\npatches = {}\n",
            "https://github.com/longbridge/gpui-kit",
            crate::copy::COPY,
            quoted(&self.release),
            quoted(&self.commit),
            quoted(&self.tree),
            array(&self.kept),
            array(&self.patches),
        );
        if let Some(settling) = &self.settling {
            let _ = write!(
                text,
                "\n# Conflicts left by the last sync; running kit-sync again checks they're settled.\n\
                 [settling]\npatch = {}\nfrom = {}\nfrom_kept = {}\nfiles = {}\n",
                quoted(&settling.patch),
                quoted(&settling.from),
                array(&settling.from_kept),
                array(&settling.files),
            );
        }
        let path = root.join(RECORD);
        fs::write(&path, text).at(&path)
    }
}

fn quoted(item: &impl ToString) -> String {
    toml::Value::String(item.to_string()).to_string()
}

fn array(items: &[impl ToString]) -> String {
    if items.is_empty() {
        return "[]".to_string();
    }
    let quoted: Vec<String> = items.iter().map(quoted).collect();
    let body: String = quoted.iter().flat_map(|item| ["    ", item.as_str(), ",\n"]).collect();
    format!("[\n{body}]")
}
