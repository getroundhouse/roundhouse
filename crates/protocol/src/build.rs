//! Which build this is. An install or a package stamps a build with `ROUNDHOUSE_BUILD` and `ROUNDHOUSE_BUILD_NUMBER`,
//! and any other build says `-dev`. A package's build version is its published version, `0.1.N`, and an install's its
//! commit.

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

/// This build, if it was stamped. One stamped with only one of the two, or with a version [`StampedBuild::new`]
/// refuses, doesn't compile.
pub const STAMPED_BUILD: Option<StampedBuild> =
    match (option_env!("ROUNDHOUSE_BUILD"), option_env!("ROUNDHOUSE_BUILD_NUMBER")) {
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
    };

/// Which build this is, for telling people.
pub const BUILD_VERSION: &str = match STAMPED_BUILD {
    Some(build) => build.version,
    None => concat!(env!("CARGO_PKG_VERSION"), "-dev"),
};

/// Puts builds in order, which a commit hash can't, so the app can tell a hub is older than it. A build with no
/// number is never replaced.
pub const BUILD_NUMBER: Option<u64> = match STAMPED_BUILD {
    Some(build) => Some(build.number),
    None => None,
};

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
}
