use std::fmt;

/// Each kind of job the hub does, for its name in the log, how many of it may run at once, and what one asked for
/// again does. Jobs of a kind are counted in the [`Jobs`](super::Jobs) they run in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum JobKind {
    /// Reading the pull requests of the workspaces due as one token, and those their waits watch, which nobody asked
    /// for, in their turn.
    ReadOwnPullRequests,
    /// Asking GitHub after a sign-in with GitHub until it's approved or ends, and keeping what it gave.
    WaitForGithubSignIn,
    /// Asking an agent that discovers its own models what it offers in one place, as a `ModelList` says.
    LookUpModels,
    /// Reading a workspace's changes from its worker, and keeping its lines and its changed files from what it says.
    ReadWorkspaceChanges,
}

/// What becomes of a job asked for while one of its kind about the same key runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AskedAgain {
    /// It isn't started, as [`NotStarted::KeyBusy`](super::NotStarted::KeyBusy) says.
    Refused,
    /// It starts once the one running has ended, however often it was asked for meanwhile, as the last one asked for.
    RunsOnceMore,
}

impl JobKind {
    /// How many of the kind may run at once.
    #[expect(clippy::match_same_arms, reason = "each kind's number has a reason of its own, which two can come to")]
    pub(super) fn at_once(self) -> usize {
        match self {
            // One for each token, of which a hub has a few, and each reads every workspace due as it together, so a
            // token's read that hangs holds up no other's. Each starts at most one worker, for a machine's token.
            Self::ReadOwnPullRequests => 4,
            // One person signs in once, so a few let a second client, or a second account, sign in at once without
            // letting a client that keeps starting them have the hub ask GitHub after many, each every five seconds
            // or so, for its code's fifteen minutes. A sign-in's job ends with it, so this is how many can wait.
            // *Unconfirmed.*
            Self::WaitForGithubSignIn => 4,
            // Each starts the agent's CLI, which takes a few seconds and some memory, and a list is asked for once per
            // place, so a few cover the dialog and the chats of the projects open, and a client that keeps asking about
            // more can't have the hub start many.
            Self::LookUpModels => 4,
            // As many as there are workspaces. Each waits on its own workspace's worker, which does the work, so one
            // that hangs holds up no other, and a read refused would leave its workspace's changes as of an older read
            // until something next said they may have changed, as each worker the hub takes on as it starts does.
            Self::ReadWorkspaceChanges => usize::MAX,
        }
    }

    /// What becomes of a job of the kind asked for while one of it about the same key runs.
    #[expect(clippy::match_same_arms, reason = "each kind's answer has a reason of its own, which two can come to")]
    pub(super) fn asked_again(self) -> AskedAgain {
        match self {
            // Whatever decides what's due passes over a token whose read is running, and lists again what's due as
            // its turn next comes, which takes in whatever changed during the read.
            Self::ReadOwnPullRequests => AskedAgain::Refused,
            // A sign-in's job is about its own ID, so another about the same ID would ask GitHub after the same code
            // twice.
            Self::WaitForGithubSignIn => AskedAgain::Refused,
            // A list says it's asking while its lookup runs, so nothing asks for another meanwhile, and whatever waits
            // for the list is told by the one running.
            Self::LookUpModels => AskedAgain::Refused,
            // The read under way may have begun before what said the changes may have changed, so it can't stand for
            // the one asked for meanwhile, which reads them from the worker the last one asked named.
            Self::ReadWorkspaceChanges => AskedAgain::RunsOnceMore,
        }
    }
}

impl fmt::Display for JobKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ReadOwnPullRequests => "reading workspaces' pull requests in turn",
            Self::WaitForGithubSignIn => "waiting for a sign-in with GitHub",
            Self::LookUpModels => "asking an agent which models it offers",
            Self::ReadWorkspaceChanges => "reading a workspace's changes from its worker",
        })
    }
}
