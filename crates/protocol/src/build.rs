//! Which build a program is. An install or a package stamps a build with `ROUNDHOUSE_BUILD` and
//! `ROUNDHOUSE_BUILD_NUMBER`, and any other build says `-dev`. A package's build version is its published version,
//! `0.1.N`, and an install's its commit.
//!
//! Each binary reads the stamp with [`this_build!`], and `main` passes the [`Build`] down. A library that read it would
//! change with every stamp, and every crate above it with it, so no build of a package or an install could reuse what
//! the one before it compiled, when most commits change few crates.

/// A build stamped with both a build version and a build number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StampedBuild {
    version: &'static str,
    number: u64,
}

impl StampedBuild {
    /// Refuses a version that can't name a directory on its own, since it names the directory of the build's copy of
    /// the daemon: one that's empty, `.` or `..`, or has a `/`.
    pub const fn new(version: &'static str, number: u64) -> Option<Self> {
        if is_plain_name(version) { Some(Self { version, number }) } else { None }
    }

    pub const fn version(&self) -> &'static str {
        self.version
    }

    pub const fn number(&self) -> u64 {
        self.number
    }
}

/// Which build a program is, which its `main` makes with [`this_build!`] and passes down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Build {
    stamped: Option<StampedBuild>,
}

impl Build {
    /// A build no install or package stamped, such as one `cargo` makes.
    pub const UNSTAMPED: Self = Self { stamped: None };

    /// The build a stamp makes, for [`this_build!`]. One stamped with only one of the two, or with a version
    /// [`StampedBuild::new`] refuses, doesn't compile, since the macro has it evaluated as a constant.
    #[doc(hidden)]
    pub const fn from_stamp(version: Option<&'static str>, number: Option<&'static str>) -> Self {
        Self { stamped: stamped(version, number) }
    }

    /// Its stamp, if it has one.
    pub const fn stamped(self) -> Option<StampedBuild> {
        self.stamped
    }

    /// Which build it is, for telling people.
    pub const fn version(self) -> &'static str {
        match self.stamped {
            Some(build) => build.version,
            None => concat!(env!("CARGO_PKG_VERSION"), "-dev"),
        }
    }

    /// Puts builds in order, which a commit hash can't, so the app can tell a hub is older than it. A build with no
    /// number is never replaced.
    pub const fn number(self) -> Option<u64> {
        match self.stamped {
            Some(build) => Some(build.number),
            None => None,
        }
    }
}

impl From<StampedBuild> for Build {
    fn from(stamped: StampedBuild) -> Self {
        Self { stamped: Some(stamped) }
    }
}

/// The build of the program it's expanded in, from the stamp that program was compiled with. A macro, so the stamp
/// is read where it's expanded, which is a binary, rather than here.
#[macro_export]
macro_rules! this_build {
    () => {
        const { $crate::Build::from_stamp(option_env!("ROUNDHOUSE_BUILD"), option_env!("ROUNDHOUSE_BUILD_NUMBER")) }
    };
}

const fn stamped(version: Option<&'static str>, number: Option<&'static str>) -> Option<StampedBuild> {
    match (version, number) {
        (Some(version), Some(number)) => {
            let Ok(number) = u64::from_str_radix(number, 10) else {
                panic!("ROUNDHOUSE_BUILD_NUMBER has to be a number");
            };
            match StampedBuild::new(version, number) {
                Some(build) => Some(build),
                None => panic!("ROUNDHOUSE_BUILD has to be one plain name, without a /, since it names a directory"),
            }
        }
        (None, None) => None,
        (Some(_), None) | (None, Some(_)) => {
            panic!("ROUNDHOUSE_BUILD and ROUNDHOUSE_BUILD_NUMBER are set together or not at all")
        }
    }
}

const fn is_plain_name(name: &str) -> bool {
    let mut rest = name.as_bytes();
    if matches!(rest, [] | [b'.'] | [b'.', b'.']) {
        return false;
    }
    while let [first, others @ ..] = rest {
        if *first == b'/' {
            return false;
        }
        rest = others;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_has_to_name_one_directory() {
        for version in ["abc123def456", "abc123def456-dirty-20261108120000", "0.1.0-dev", "..a", "a.."] {
            assert!(StampedBuild::new(version, 1).is_some(), "{version}");
        }
        for version in ["", ".", "..", "a/b", "/abc", "abc/", "../abc"] {
            assert_eq!(StampedBuild::new(version, 1), None, "{version}");
        }
    }

    #[test]
    fn a_stamp_names_the_build_and_its_number_and_no_stamp_says_dev() {
        let stamped = Build::from_stamp(Some("abc123"), Some("7"));
        assert_eq!((stamped.version(), stamped.number()), ("abc123", Some(7)));
        assert_eq!(stamped.stamped(), StampedBuild::new("abc123", 7));

        let unstamped = Build::from_stamp(None, None);
        assert_eq!(unstamped, Build::UNSTAMPED);
        assert_eq!((unstamped.version(), unstamped.number()), (concat!(env!("CARGO_PKG_VERSION"), "-dev"), None));
    }
}
