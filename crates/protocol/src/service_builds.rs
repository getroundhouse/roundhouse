//! Where a hub installed as a service keeps its builds, in its data directory: each build's daemon and command line in
//! a directory named for the build, beside the last, and `current`, a link to the build the service runs. The command
//! line places a build there as it installs one, and the service starts the daemon through the link, so a new build
//! takes over at the hub's next start, and a running worker's binary is never replaced under it.

use std::path::PathBuf;

use crate::StampedBuild;
use crate::paths::{Paths, daemon_file, program_file};

/// The folder beside a service's builds that an update downloads and unpacks the next in, which no build is named, and
/// which goes once the update is done, or as the next hub starts after one cut short.
pub const SERVICE_UPDATE_DIR: &str = ".update";

/// The command line's file, which it has beside the daemon in each build the service runs.
pub fn cli_file() -> String {
    program_file("roundhouse")
}

/// Each program in a build's directory for the service: the daemon, which the service and its workers run from, and
/// then the command line.
pub fn service_programs() -> [String; 2] {
    [daemon_file(), cli_file()]
}

impl Paths {
    /// Where a hub installed as a service keeps its builds.
    pub fn service_builds_dir(&self) -> PathBuf {
        self.data_dir.join("service")
    }

    /// The directory that holds `build`'s daemon and command line, for a hub installed as a service.
    pub fn service_build(&self, build: &StampedBuild) -> PathBuf {
        self.service_builds_dir().join(build.dir_name())
    }

    /// The link to the build that a hub installed as a service runs, which names that build's directory from beside it.
    pub fn service_current(&self) -> PathBuf {
        self.service_builds_dir().join("current")
    }
}

#[cfg(test)]
mod tests {
    use crate::StampedBuild;
    use crate::paths::{ENV_HOME, HOME_VAR, resolve};
    use std::path::Path;

    #[test]
    fn each_build_of_a_service_has_a_directory_beside_the_link_to_the_one_it_runs() {
        let (home, data) = if cfg!(windows) { ("C:/h", "C:/d") } else { ("/h", "/d") };
        let env = |name: &str| match name {
            HOME_VAR => Some(home.to_owned()),
            ENV_HOME => Some(data.to_owned()),
            _ => None,
        };
        let paths = resolve(env).unwrap();

        let build = paths.service_build(&StampedBuild::new("0.1.371", 1_794_519_400).unwrap());
        let current = paths.service_current();

        assert_eq!(build, Path::new(data).join("service/0.1.371-1794519400"));
        assert_eq!(build.parent(), current.parent());
    }
}
