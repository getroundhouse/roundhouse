use std::fmt;

/// Each kind of job the hub does, for its name in the log and how many of it may run at once. Jobs of a kind are counted
/// in the [`Jobs`](super::Jobs) they run in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum JobKind {
    /// Reading the pull requests of the workspaces due as one token, which nobody asked for, in their turn.
    ReadOwnPullRequests,
    /// Reading the pull requests a group of waits watch.
    ReadWaitedPullRequests,
    /// Asking GitHub after a sign-in with GitHub until it's approved or ends, and keeping what it gave.
    WaitForGithubSignIn,
    /// Asking an agent that discovers its own models what it offers in one place, as a `ModelList` says.
    LookUpModels,
}

impl JobKind {
    /// How many of the kind may run at once.
    #[expect(clippy::match_same_arms, reason = "each kind's number has a reason of its own, which two can come to")]
    pub(super) fn at_once(self) -> usize {
        match self {
            // One for each token, of which a hub has a few, and each reads every workspace due as it together, so a
            // token's read that hangs holds up no other's. Each starts at most one worker, for a machine's token.
            Self::ReadOwnPullRequests => 4,
            // An agent waits on these, so one workspace's that hangs mustn't hold up another's. Each group's read goes
            // at most every two minutes, and stops at the last fifth of its token's limit, so a few at once spend no
            // more. GitHub asks for its calls one at a time rather than many at once, so not many.
            Self::ReadWaitedPullRequests => 4,
            // One person signs in once, so a few let a second client, or a second account, sign in at once without
            // letting a client that keeps starting them have the hub ask GitHub after many, each every five seconds
            // or so, for its code's fifteen minutes. A sign-in's job ends with it, so this is how many can wait.
            Self::WaitForGithubSignIn => 4,
            // Each starts the agent's CLI, which takes a few seconds and some memory, and a list is asked for once per
            // place, so a few cover the dialog and the chats of the projects open, and a client that keeps asking about
            // more can't have the hub start many.
            Self::LookUpModels => 4,
        }
    }
}

impl fmt::Display for JobKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ReadOwnPullRequests => "reading workspaces' pull requests in turn",
            Self::ReadWaitedPullRequests => "reading the pull requests waits watch",
            Self::WaitForGithubSignIn => "waiting for a sign-in with GitHub",
            Self::LookUpModels => "asking an agent which models it offers",
        })
    }
}
