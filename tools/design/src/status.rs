//! Where a part of the design stands, as the badge at the top of its canvas says: a copy of one of the masters in
//! `Board/Status`, by name.

use std::fmt;

/// Where a part stands, as `docs/standards/design.md` has it under "Deciding".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Being worked on, and not to be built.
    Exploring,
    /// The agent thinks it's settled, and the owner hasn't said so.
    ReadyForReview,
    /// The owner settled it, and it can be built.
    Ready,
}

/// The name of a canvas's own badge, which no other copy of a status's master on it has.
pub(crate) const BADGE: &str = "Status";

impl Status {
    pub const ALL: [Status; 3] = [Status::Exploring, Status::ReadyForReview, Status::Ready];

    /// The name of the master in `Board/Status` that a badge of this status is a copy of.
    pub fn master(self) -> &'static str {
        match self {
            Status::Exploring => "Status Exploring",
            Status::ReadyForReview => "Status Ready for review",
            Status::Ready => "Status Ready",
        }
    }

    /// The status of `row`, a row of a canvas, if it's the canvas's own badge: a copy of a status's master that's named
    /// [`BADGE`]. A copy named otherwise, such as one in an option of a `To decide` section, isn't the canvas's badge.
    pub fn of_badge(row: &str) -> Option<Status> {
        let copied = row.trim_end().strip_suffix(&format!(" \"{BADGE}\""))?.split_once("inst(\"")?.1;
        Status::ALL.into_iter().find(|status| copied.starts_with(&format!("{}\"", status.master())))
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.master())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_is_the_canvass_badge_when_it_copies_a_statuss_master_and_is_named_as_the_badge() {
        assert_eq!(
            Status::of_badge("  2 inst(\"Status Ready for review\",canvas(Board/Status)) p(0,0) \"Status\""),
            Some(Status::ReadyForReview)
        );
        assert_eq!(
            Status::of_badge("1 inst(\"Status Ready\",canvas(Board/Status)) \"Option A\""),
            None,
            "named otherwise"
        );
        assert_eq!(
            Status::of_badge("1 inst(\"Card\",canvas(Components/Card)) \"Status\""),
            None,
            "not a status's master"
        );
    }

    #[test]
    fn a_master_whose_name_only_starts_as_a_statuss_is_not_a_badge() {
        assert_eq!(Status::of_badge("1 inst(\"Status Ready later\") \"Status\""), None);
    }
}
