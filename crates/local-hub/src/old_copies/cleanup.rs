use super::super::temporary_prefix;
use protocol::StampedBuild;
use protocol::paths::{Paths, hub_copy_files};

/// Removes each copy of the daemon in `paths` that no process runs from, as [`process::Builds::remove_unused`] says,
/// but `own`'s, with the directory it was in.
pub(in super::super) fn remove_unused(paths: &Paths, own: &StampedBuild) {
    let own = paths.hub_copy(own).parent().map(Into::into);
    let copies = process::Builds {
        dir: &paths.daemons_dir,
        is_build: StampedBuild::is_dir_name,
        programs: &hub_copy_files(),
        temporary_prefix: &temporary_prefix(),
    };
    copies.remove_unused(&Vec::from_iter(own));
}
