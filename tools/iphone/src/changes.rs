//! Whether the app changed since an upload, and what `TestFlight` tells testers changed. The `TestFlight` workflow uploads
//! on a schedule only when the app changed since the last run that passed.

use std::fmt::Write as _;
use std::path::Path;

use crate::{cargo, git};

/// Besides the app's crates: the versions they're built at, and how Cargo and Rust build them.
const BUILT_WITH: [&str; 4] = ["Cargo.lock", "Cargo.toml", "rust-toolchain.toml", ".cargo"];

/// Whether what the app is built from differs between `commit` and HEAD: the crates it builds for an iPhone, as
/// HEAD's `Cargo.lock` has them, which include its own crate and so its Xcode project, and what they're built with.
pub(crate) fn changed_since(root: &Path, commit: &str) -> anyhow::Result<bool> {
    git::differs(root, commit, &sources(root)?)
}

/// What `TestFlight` tells testers to try: the commit it was built from, and the titles of the commits since `since`
/// that changed what the app is built from.
pub(crate) fn what_to_test(root: &Path, since: Option<&str>) -> anyhow::Result<String> {
    let head = git::head(root)?;
    let titles = match since {
        Some(since) => Some(git::titles(root, since, &sources(root)?)?),
        None => None,
    };
    Ok(notes(&head, titles.as_deref()))
}

fn sources(root: &Path) -> anyhow::Result<Vec<String>> {
    let mut sources = cargo::app_crates(root)?;
    sources.extend(BUILT_WITH.map(str::to_string));
    Ok(sources)
}

/// The notes for a build of `head`, with the titles of what changed since the last build, if that's known.
fn notes(head: &str, titles: Option<&[String]>) -> String {
    let short = head.get(..12).unwrap_or(head);
    let mut notes = format!("Built from {short} on main.\n");
    match titles {
        None => {}
        Some([]) => notes.push_str("\nNothing the app is built from changed since the last build.\n"),
        Some(titles) => {
            notes.push_str("\nWhat changed for the app since the last build:\n");
            for title in titles {
                let _ = writeln!(notes, "- {title}");
            }
        }
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_notes_say_the_commit_then_what_changed_since_the_last_build_if_its_known() {
        let head = "0123456789abcdef0123456789abcdef01234567";
        let titles = ["Show a chat's draft (#2346)".to_string(), "Update every crate (#2350)".to_string()];
        let cases: [(Option<&[String]>, &str); 3] = [
            (None, "Built from 0123456789ab on main.\n"),
            (
                Some(&[]),
                "Built from 0123456789ab on main.\n\nNothing the app is built from changed since the last build.\n",
            ),
            (
                Some(&titles),
                "Built from 0123456789ab on main.\n\nWhat changed for the app since the last build:\n\
                 - Show a chat's draft (#2346)\n- Update every crate (#2350)\n",
            ),
        ];

        for (titles, expected) in cases {
            assert_eq!(notes(head, titles), expected);
        }
    }
}
