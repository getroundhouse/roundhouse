//! What a build of the app says it is: the version people see, and the build number App Store Connect orders a
//! version's builds by.

use std::fmt;
use std::path::Path;
use std::process::Command;

use crate::program;

/// The desktop's version for the same commit, `0.1.<commits>`, as `docs/decisions/publish-every-merge.md` says,
/// which `tools/build.sh` works out for every build, so it's asked rather than worked out again here.
pub(crate) struct Version(String);

impl Version {
    /// It counts the commits, so it needs the whole history, and refuses a shallow clone.
    pub(crate) fn of(root: &Path) -> anyhow::Result<Self> {
        let printed = program::output(Command::new(root.join("tools/package/package.sh")).arg("--print-version"))?;
        Ok(Self(printed.trim().to_string()))
    }

    /// For a phone of one's own, which a shallow checkout can build too.
    pub(crate) fn for_a_phone(root: &Path) -> Self {
        Self::of(root).unwrap_or_else(|_| Self("0.1.0".to_string()))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// When the build was made, in UTC, as year-month-day.hour-minute, each part read as a number, so the time loses its
/// leading zeros: 05:55 on 22 November 2026 is `20261122.555`. Each upload's has to be higher than the last one's. A
/// commit count isn't, from a branch with more commits than main, so it's the time, and uploads are a minute apart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BuildNumber(String);

impl BuildNumber {
    pub(crate) fn at(when: jiff::Timestamp) -> Self {
        let utc = when.to_zoned_datetime(jiff::tz::TimeZone::UTC);
        let minutes = i32::from(utc.hour()) * 100 + i32::from(utc.minute());
        Self(format!("{:04}{:02}{:02}.{minutes}", utc.year(), utc.month(), utc.day()))
    }
}

impl fmt::Display for BuildNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_number_is_the_utc_minute_with_its_leading_zeros_gone() {
        let cases = [
            ("2026-11-22T05:55:30Z", "20261122.555"),
            ("2026-01-02T00:07:00Z", "20260102.7"),
            ("2026-12-31T23:59:59Z", "20261231.2359"),
            ("2026-11-22T00:00:00-08:00", "20261122.800"),
        ];

        for (when, expected) in cases {
            let number = BuildNumber::at(when.parse().unwrap());

            assert_eq!(number.to_string(), expected, "{when}");
        }
    }
}
