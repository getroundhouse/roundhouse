//! Brings the copy and its patches to the release a pin names, or makes the copy.

use std::path::Path;

use crate::error::Error;
use crate::files::CopyPath;
use crate::git::Git;
use crate::merge::{first_copy, move_release, resume_settling};
use crate::patches::PatchName;
use crate::pin::Pin;
use crate::record::Record;
use crate::upstream::tagged;
use crate::verify::verify_against_record;
use crate::versions::{Versions, pin_versions};

/// How a sync left the copy and its patches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Synced {
    /// Made, or brought onto the release with our patches updated. `moved` holds the versions `Cargo.toml` now has, if
    /// they changed, and `dropped` names any patches whose changes upstream already has.
    Brought { moved: Option<Versions>, dropped: Vec<PatchName> },
    /// On the release already, as the record says, which is written again in case the release's tag is another.
    Already { moved: Option<Versions> },
    /// Stopped on `patch`, whose conflicts in `files` have to be settled in the copy before running [`sync`] again.
    Unsettled { patch: PatchName, files: Vec<CopyPath> },
}

/// Brings the copy and our patches in the repository at `root` to the release `pin` names, updating each patch in
/// [`crate::PATCHES`] onto the release and moving the versions in `Cargo.toml` to the copy's. With no copy yet, it
/// makes one. Run again once a patch's conflicts are settled in the copy, it writes that patch and continues with the
/// rest.
pub fn sync(git: &Git, root: &Path, pin: &Pin) -> Result<Synced, Error> {
    let tags = tagged(git, root, &pin.upstream, &pin.release)?;
    if tags.as_ref() != Some(&pin.commit) {
        return Err(Error::TagElsewhere { release: pin.release.clone(), tagged: tags, pinned: pin.commit.clone() });
    }
    let Some(mut made) = Record::read(root)? else {
        return first_copy(git, root, pin);
    };
    if let Some(settling) = made.settling.clone() {
        return resume_settling(git, root, pin, &made, settling);
    }
    verify_against_record(git, root, &made)?;
    if made.commit == pin.commit && made.kept == pin.kept {
        made.release = pin.release.clone();
        made.write(root)?;
        return Ok(Synced::Already { moved: pin_versions(root)? });
    }
    move_release(git, root, pin, &made)
}
