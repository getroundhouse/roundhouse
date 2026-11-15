//! What the copy was made from, which [`crate::sync`] writes beside it, and reads to merge the next release with our
//! changes.

use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::error::{At, Error};
use crate::files::{CopyPath, read_if_there};
use crate::pin::{Commit, Kept, Release};

/// Where the record is, from the root of this repository.
pub const RECORD: &str = "third_party/gpui-kit.toml";

/// The release and commit of upstream's that the copy was made from, and what it kept of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub release: Release,
    pub commit: Commit,
    pub kept: Vec<Kept>,
    /// The files the last merge left with a conflict, which have to be settled before the copy counts as on the
    /// release.
    pub unsettled: Vec<CopyPath>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Written {
    release: Release,
    commit: Commit,
    kept: Vec<String>,
    #[serde(default)]
    unsettled: Vec<CopyPath>,
}

impl Record {
    /// The record in the repository at `root`, or `None` when there isn't one.
    pub fn read(root: &Path) -> Result<Option<Self>, Error> {
        let path = root.join(RECORD);
        let Some(text) = read_if_there(&path)? else { return Ok(None) };
        let written: Written = toml::from_str(&text).map_err(|source| Error::Toml { path, source })?;
        Ok(Some(Self {
            release: written.release,
            commit: written.commit,
            kept: written.kept.iter().map(|path| Kept::from(path.as_str())).collect(),
            unsettled: written.unsettled,
        }))
    }

    pub(crate) fn write(&self, root: &Path) -> Result<(), Error> {
        let quoted = |text: &dyn ToString| toml::Value::String(text.to_string()).to_string();
        let list = |items: Vec<String>| items.iter().flat_map(|item| ["    ", item, ",\n"]).collect::<String>();
        let mut text = format!(
            "# The release of {} that {} was made from, and what it\n\
             # kept of it, which tools/kit-sync writes, and reads to merge the next release with our changes.\n\
             release = {}\ncommit = {}\nkept = [\n{}]\n",
            "https://github.com/longbridge/gpui-kit",
            crate::copy::COPY,
            quoted(&self.release),
            quoted(&self.commit),
            list(self.kept.iter().map(|path| quoted(path)).collect()),
        );
        if !self.unsettled.is_empty() {
            text += "# Conflicts left by the last merge, which running kit-sync again checks are settled.\n";
            text.extend(["unsettled = [\n", &list(self.unsettled.iter().map(|path| quoted(path)).collect()), "]\n"]);
        }
        let path = root.join(RECORD);
        fs::write(&path, text).at(&path)
    }
}
