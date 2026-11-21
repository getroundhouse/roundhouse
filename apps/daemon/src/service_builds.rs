//! The builds a hub installed as a service runs from, which each update leaves another of, beside the last.

use protocol::StampedBuild;
use protocol::paths::{Paths, SERVICE_UPDATE_DIR, service_programs};

/// Removes each build in the service's directory that nothing runs from, as [`process::Builds::remove_unused`] says,
/// but `own` and the one the service's link names, which may have been placed since this hub started, and what an
/// update cut short left. A hub of the service starts once the one before has gone, which is when a build goes out of
/// use and no update is under way, so it removes them as it starts.
pub(crate) fn remove_unused(paths: &Paths, own: &StampedBuild) {
    let builds = paths.service_builds_dir();
    let _ = std::fs::remove_dir_all(builds.join(SERVICE_UPDATE_DIR));
    let mut keep = vec![paths.service_build(own)];
    // Named from beside it, as placing a build links it.
    keep.extend(std::fs::read_link(paths.service_current()).ok().map(|named| builds.join(named)));
    let service = process::Builds {
        dir: &builds,
        is_build: StampedBuild::is_dir_name,
        programs: &service_programs(),
        // Each program goes in under a hidden name first, as placing a build does.
        temporary_prefix: ".",
    };
    service.remove_unused(&keep);
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::{Duration, SystemTime};

    use super::*;

    /// Longer ago than a build is kept with nothing running from it.
    const LONG_AGO: Duration = Duration::from_hours(48);

    fn paths_in(dir: &Path) -> Paths {
        let home = dir.join("home").display().to_string();
        let data = dir.join("data").display().to_string();
        protocol::paths::resolve(|name| match name {
            protocol::paths::HOME_VAR => Some(home.clone()),
            protocol::paths::ENV_HOME => Some(data.clone()),
            _ => None,
        })
        .unwrap()
    }

    /// Puts `build`'s programs in its directory, as placing it does, made `ago`.
    fn placed(paths: &Paths, build: &StampedBuild, ago: Duration) {
        let dir = paths.service_build(build);
        std::fs::create_dir_all(&dir).unwrap();
        for program in service_programs() {
            let file = std::fs::File::create(dir.join(program)).unwrap();
            file.set_modified(SystemTime::now() - ago).unwrap();
        }
    }

    fn build(number: u64) -> StampedBuild {
        StampedBuild::new("0.1.7", number).unwrap()
    }

    /// Points the service at `build`, as placing it does.
    fn link_current(paths: &Paths, build: &StampedBuild) {
        #[cfg(unix)]
        std::os::unix::fs::symlink(build.dir_name(), paths.service_current()).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(build.dir_name(), paths.service_current()).unwrap();
    }

    fn left(paths: &Paths) -> Vec<String> {
        let mut names: Vec<_> = std::fs::read_dir(paths.service_builds_dir())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// A worker of an earlier build marks the daemon it runs from, which keeps that build's command line too.
    #[test]
    fn a_build_goes_once_nothing_runs_from_it_but_this_ones_and_the_one_the_service_names() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_in(dir.path());
        for number in 1..=5 {
            placed(&paths, &build(number), LONG_AGO);
        }
        link_current(&paths, &build(5));
        let worker = paths.service_build(&build(2)).join(protocol::paths::daemon_file());
        let _running = process::RunningFrom::mark(&worker).unwrap();

        remove_unused(&paths, &build(4));

        assert_eq!(left(&paths), ["0.1.7-2", "0.1.7-4", "0.1.7-5", "current"]);
        let kept: Vec<_> =
            service_programs().iter().map(|program| paths.service_build(&build(2)).join(program)).collect();
        assert!(kept.iter().all(|program| program.exists()), "{kept:?}");
    }

    /// Such as one an install placed just before it was run.
    #[test]
    fn a_build_placed_within_a_day_stays() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_in(dir.path());
        placed(&paths, &build(1), LONG_AGO);
        placed(&paths, &build(2), Duration::from_hours(1));
        placed(&paths, &build(3), LONG_AGO);

        remove_unused(&paths, &build(3));

        assert_eq!(left(&paths), ["0.1.7-2", "0.1.7-3"]);
    }
}
