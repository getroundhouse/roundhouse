use std::fmt;

/// Each kind of job the hub does, for its name in the log and how many of it may run at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum JobKind {
    /// Reading a workspace's pull request nobody asked for, in its turn.
    ReadPullRequestInTurn,
    /// Reading the pull requests a group of waits watch.
    ReadWaitedPullRequests,
}

impl JobKind {
    /// How many of the kind may run at once.
    pub(super) fn at_once(self) -> usize {
        match self {
            // One at a time, as `docs/spec/workspaces.md` has reads nobody is waiting on, so they start at most one
            // worker at once and keep to the pace the budget of calls to GitHub was set for. A read's limit bounds how
            // long one that hangs holds up the next.
            Self::ReadPullRequestInTurn => 1,
            // An agent waits on these, so one workspace's that hangs mustn't hold up another's. Each group's read goes
            // at most every two minutes, and takes its calls from the budget as it goes, so a few at once spend no
            // more. GitHub asks for its calls one at a time rather than many at once, so not many.
            Self::ReadWaitedPullRequests => 4,
        }
    }
}

impl fmt::Display for JobKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ReadPullRequestInTurn => "reading a workspace's pull request in turn",
            Self::ReadWaitedPullRequests => "reading the pull requests waits watch",
        })
    }
}
