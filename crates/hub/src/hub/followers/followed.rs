use protocol::model::{Level, ReferenceAsk, SessionId, TerminalId, WorkspaceId};

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
    /// A workspace, as a client follows it while it shows the workspace, whose pull request the hub reads more often,
    /// and whose worker, if it's running, watches its worktree.
    Workspace(WorkspaceId),
    /// A session's log. Nothing is kept current for it alone, since its agent says what changes whether or not
    /// anything follows it.
    Session(SessionId),
    /// A terminal, whose worker sends what it shows only while something follows it.
    Terminal(TerminalId),
    /// An issue or a pull request, as a chip shows it, which the hub reads again once it's stale.
    Reference(ReferenceAsk),
}

impl Followed {
    /// The level it's followed at.
    pub(crate) fn level(&self) -> Level {
        match self {
            Followed::Hub => Level::Hub,
            Followed::Workspace(id) => Level::Workspace { id: id.clone() },
            Followed::Session(id) => Level::Session { id: id.clone() },
            Followed::Terminal(id) => Level::Terminal { id: id.clone() },
            Followed::Reference(reference) => Level::Reference { reference: reference.clone() },
        }
    }
}

impl TryFrom<&Level> for Followed {
    /// The level is from a newer client, so the hub doesn't know it.
    type Error = ();

    fn try_from(level: &Level) -> Result<Self, ()> {
        Ok(match level {
            Level::Hub => Followed::Hub,
            Level::Workspace { id } => Followed::Workspace(id.clone()),
            Level::Session { id } => Followed::Session(id.clone()),
            Level::Terminal { id } => Followed::Terminal(id.clone()),
            Level::Reference { reference } => Followed::Reference(reference.clone()),
            Level::Unknown => return Err(()),
        })
    }
}
