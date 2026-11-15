//! Brings the copy to the release a pin names, or makes it.

use std::fs;
use std::path::Path;

use crate::copy::COPY;
use crate::error::{At, Error};
use crate::files::{CopyPath, marked};
use crate::git::Git;
use crate::merge::brought;
use crate::pin::Pin;
use crate::record::Record;
use crate::upstream::tagged;
use crate::versions::{Versions, pin_versions};

/// How a sync left the copy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Synced {
    /// Made, or merged with upstream's changes, onto the release. `moved` holds the versions `Cargo.toml` now has, if
    /// it didn't have them already.
    Brought { moved: Option<Versions> },
    /// On the release already, as the record says, which is written again in case the release's tag is another.
    Already { moved: Option<Versions> },
    /// Merged, with these files left as git leaves a conflict, which have to be settled before it counts as on the
    /// release. `Cargo.toml` is left as it was.
    Unsettled(Vec<CopyPath>),
}

/// Brings the copy in the repository at `root` to the release `pin` names, merging what upstream changed since the
/// release it's on with what we changed, as git merges two branches, then moves the versions in `Cargo.toml` to the
/// copy's. With no copy yet, it makes one. Run again once a merge's conflicts are settled, it checks that none is
/// still marked.
pub fn sync(git: &Git, root: &Path, pin: &Pin) -> Result<Synced, Error> {
    let tags = tagged(git, root, &pin.upstream, &pin.release)?;
    if tags.as_ref() != Some(&pin.commit) {
        return Err(Error::TagElsewhere { release: pin.release.clone(), tagged: tags, pinned: pin.commit.clone() });
    }
    let made = Record::read(root)?;
    let copy = root.join(COPY);
    let mut still = Vec::new();
    for path in made.iter().flat_map(|made| &made.unsettled) {
        if marked(&path.under(&copy))? {
            still.push(path.clone());
        }
    }
    if !still.is_empty() {
        return Err(Error::StillMarked(still));
    }
    let moving = made.as_ref().is_none_or(|made| made.commit != pin.commit || made.kept != pin.kept);
    let conflicts = if moving {
        // In the repository's `target`, which git ignores and `cargo clean` reaches.
        let scratch = root.join("target");
        fs::create_dir_all(&scratch).at(&scratch)?;
        let work = tempfile::Builder::new().prefix("kit-sync-").tempdir_in(&scratch).at(&scratch)?;
        brought(git, root, pin, made.as_ref(), work.path())?
    } else {
        Vec::new()
    };
    let unsettled = conflicts.clone();
    Record { release: pin.release.clone(), commit: pin.commit.clone(), kept: pin.kept.clone(), unsettled }
        .write(root)?;
    if !conflicts.is_empty() {
        return Ok(Synced::Unsettled(conflicts));
    }
    // Only from a manifest with nothing left to settle, which running this again once it's settled reads.
    let moved = pin_versions(root)?;
    Ok(if moving { Synced::Brought { moved } } else { Synced::Already { moved } })
}
