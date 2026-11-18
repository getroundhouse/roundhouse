use protocol::paths::Paths;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Gives each link this process makes one of its own name, with its ID.
static NEXT_LINKING: AtomicU64 = AtomicU64::new(0);

/// Puts `copy` at [`Paths::fixed_hub`] and returns that path. It links the copy under a name of its own and then
/// renames that over the fixed path, so the fixed path always holds a whole hub and never goes missing, even while a
/// client starts one from it. Windows lets a rename replace a program that's running. It's placed again each time,
/// which is only when a hub starts, rather than first finding out whether it's there already. Whatever was there before
/// is never written to, and stays a build's copy.
///
/// A hub that's running there goes on running its own build, and starts its workers from its build's copy, so they're
/// never another build's. Another client starting a hub at once can put its own build there after this one has, which
/// is the hub that this one then starts, as it is when two clients race to start one anyway.
pub(in super::super) fn place(paths: &Paths, copy: &Path) -> io::Result<PathBuf> {
    let fixed = paths.fixed_hub();
    let linking = paths.linking_hub(NEXT_LINKING.fetch_add(1, Ordering::Relaxed));
    std::fs::hard_link(copy, &linking)
        .map_err(|e| io::Error::new(e.kind(), format!("linking {} to {}: {e}", copy.display(), linking.display())))?;
    std::fs::rename(&linking, &fixed).map_err(|e| {
        let _ = std::fs::remove_file(&linking);
        io::Error::new(e.kind(), format!("putting {} at {}: {e}", copy.display(), fixed.display()))
    })?;
    Ok(fixed)
}

/// Removes each link [`place`] left under its own name, as one killed between making it and renaming it does, unless
/// a process runs from it. A link's file is a build's copy too, so removing it removes nothing else. One that another
/// client is placing just then can go as well, which only has that client start its hub from its build's copy.
pub(in super::super) fn remove_leftovers(paths: &Paths) {
    let Ok(entries) = std::fs::read_dir(&paths.daemons_dir) else { return };
    let prefix = protocol::paths::linking_hub_prefix();
    for path in entries.filter_map(|entry| Some(entry.ok()?.path())) {
        let leftover = path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.starts_with(&prefix));
        // It's removed while it's claimed, so nothing starting from it can mark it first.
        if leftover && let Ok(Some(_claimed)) = process::claim_unused(&path) {
            let _ = std::fs::remove_file(&path);
        }
    }
}
