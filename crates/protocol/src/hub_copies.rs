//! The names the hub's copies of the daemon have in [`Paths::daemons_dir`]: each build's copy, in a directory named for
//! the build, and on Windows the fixed path every build's hub runs from, with the names a copy is linked to on its way
//! there. `paths` gives them out with the rest.

use std::path::PathBuf;

use crate::StampedBuild;
use crate::paths::{HUB_COPY_NAME, Paths, daemon_file, program_file};

impl Paths {
    /// The copy of the daemon that `build`'s hubs run from, called `Roundhouse Hub`, in a directory named for the build.
    pub fn hub_copy(&self, build: &StampedBuild) -> PathBuf {
        self.daemons_dir.join(build.dir_name()).join(program_file(HUB_COPY_NAME))
    }

    /// Where a Windows hub runs from, whichever build it is: a second name for the build's [`Paths::hub_copy`], which
    /// the client that starts the hub links there each time. The firewall gives a program leave to listen by its path,
    /// so the leave given once outlasts every update. The workers that hub starts run from its build's copy, as
    /// `workers_from` in the daemon's `hub_role` says.
    pub fn fixed_hub(&self) -> PathBuf {
        self.daemons_dir.join(program_file(HUB_COPY_NAME))
    }

    /// A name, with this process's ID, that a build's copy is linked to before it's renamed over [`Paths::fixed_hub`],
    /// beside it. `n` keeps each of this process's apart.
    pub fn linking_hub(&self, n: u64) -> PathBuf {
        self.daemons_dir.join(format!(
            "{}{}.{n}{}",
            linking_hub_prefix(),
            std::process::id(),
            std::env::consts::EXE_SUFFIX
        ))
    }
}

/// What every name [`Paths::linking_hub`] gives starts with, so it's hidden, and never the name of anything else.
pub fn linking_hub_prefix() -> String {
    format!(".{HUB_COPY_NAME}.linking.")
}

/// Each name a build's copy can have in its directory: the one [`Paths::hub_copy`] gives it, and the daemon's, which
/// builds from before copies were called `Roundhouse Hub` gave theirs.
pub fn hub_copy_files() -> [String; 2] {
    [program_file(HUB_COPY_NAME), daemon_file()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::{ENV_HOME, HOME_VAR, resolve};
    use std::path::Path;

    /// Paths with a whole data directory on the platform the test runs on.
    fn paths() -> (Paths, String) {
        let (home, data) = if cfg!(windows) { ("C:/h", "C:/d") } else { ("/h", "/d") };
        let env = |name: &str| match name {
            HOME_VAR => Some(home.to_owned()),
            ENV_HOME => Some(data.to_owned()),
            _ => None,
        };
        (resolve(env).unwrap(), data.to_owned())
    }

    #[test]
    fn a_hub_copy_is_called_roundhouse_hub_in_a_directory_named_for_its_build() {
        let (paths, data) = paths();

        let copy = paths.hub_copy(&StampedBuild::new("abc123", 7).unwrap());

        let file = format!("Roundhouse Hub{}", std::env::consts::EXE_SUFFIX);
        assert_eq!(copy, Path::new(&data).join("daemons/abc123-7").join(file));
    }

    #[test]
    fn the_fixed_hub_is_called_roundhouse_hub_beside_the_builds_directories() {
        let (paths, _) = paths();
        let copy = paths.hub_copy(&StampedBuild::new("abc123", 7).unwrap());

        let fixed = paths.fixed_hub();
        let linking = paths.linking_hub(3);

        assert_eq!(fixed.parent().unwrap(), copy.parent().unwrap().parent().unwrap());
        assert_eq!(fixed.file_name(), copy.file_name());
        assert_eq!(linking.parent(), fixed.parent());
        let name = linking.file_name().unwrap().to_str().unwrap();
        assert!(name.starts_with(&linking_hub_prefix()), "{name}");
        assert!(!StampedBuild::is_dir_name(name), "{name}");
    }

    #[test]
    fn a_hub_copy_s_directory_is_told_from_any_other_name() {
        let (paths, _) = paths();
        let copy = paths.hub_copy(&StampedBuild::new("0.1.371", 1_794_519_400).unwrap());
        let dir = copy.parent().unwrap().file_name().unwrap().to_str().unwrap();

        assert!(StampedBuild::is_dir_name(dir));
        for other in ["notes", "abc123-new", "-7", "abc123-07", "abc123-+7"] {
            assert!(!StampedBuild::is_dir_name(other), "{other}");
        }
    }
}
