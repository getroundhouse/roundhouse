use protocol::model::{ReferenceAsk, SessionId, TerminalId, WorkspaceId};

/// Which subscription of which connection follows something.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct FollowerId {
    pub(crate) connection: u64,
    pub(crate) subscription: u64,
}

/// What a client follows, at one of the levels `docs/principles/serving.md` names, as the hub counts who follows it. It's
/// a `protocol::model::Level` that the hub knows, so nothing in the list is from a newer client.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Followed {
    /// The hub's records, as `FeedSubscribe` follows them. While anything follows it, the hub reads again the pull
    /// requests of archived workspaces that its cache has forgotten, as `Hub::read_forgotten_followed` says.
    Hub,
    /// A workspace, as a client follows it while it shows the workspace's pull request in full, which the hub reads
    /// more often.
    Workspace(WorkspaceId),
    /// A session's log. Nothing is kept current for it alone, since its agent says what changes whether or not
    /// anything follows it.
    Session(SessionId),
    /// A terminal, whose worker sends what it shows only while something follows it.
    Terminal(TerminalId),
    /// An issue or a pull request, as a chip shows it, which the hub reads again once it's stale.
    Reference(ReferenceAsk),
}
