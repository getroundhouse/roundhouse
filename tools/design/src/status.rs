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

    /// The status of the first badge in `text`, a canvas or its rows: the first copy of a status's master.
    pub fn first_in(text: &str) -> Option<Status> {
        text.split("inst(\"").skip(1).find_map(|after| {
            Status::ALL.into_iter().find(|status| after.starts_with(&format!("{}\"", status.master())))
        })
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
    fn a_canvas_has_the_status_of_its_first_badge() {
        let canvas = "1 inst(\"Card\") \"Card\"\n2 inst(\"Status Ready for review\",canvas(Board/Status)) \"Status\"\n3 inst(\"Status Ready\")";

        assert_eq!(Status::first_in(canvas), Some(Status::ReadyForReview));
    }

    #[test]
    fn a_master_whose_name_only_starts_as_a_statuss_is_not_a_badge() {
        assert_eq!(Status::first_in("1 inst(\"Status Ready later\")"), None);
    }
}
