use serde::{Deserialize, Serialize};

/// Declares ID types: text that's only compared and passed along, with a type each so that one
/// can't be passed as another. They're plain strings on the wire and in storage.
macro_rules! ids {
    ($($(#[$doc:meta])* $name:ident),+ $(,)?) => {$(
        $(#[$doc])*
        #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<String> for $name {
            fn from(id: String) -> Self {
                Self(id)
            }
        }

        impl From<&str> for $name {
            fn from(id: &str) -> Self {
                Self(id.to_string())
            }
        }
    )+};
}
pub(crate) use ids;

ids!(
    /// An account an agent runs under, as its adapter names it. It's opaque, says which kind of agent it's for, and
    /// is the same for every agent under the same login, so the hub can keep what's the account's, such as its
    /// limits, in one place.
    AccountId,
    RepoId,
    WorkspaceId,
    SessionId,
    CommentId,
    /// The agent's own ID for an approval it asked for.
    ApprovalId,
    /// Where a workspace's worker runs, in its provider's terms.
    InstanceId,
    /// One of a hub's providers, by the name the hub has it under, such as `local`, which each workspace records.
    ProviderName,
    /// A hub's own ID, made once and kept, so a provider adapter serving several hubs on one host works only with
    /// the instances it made for the one that's asking.
    HubId,
    /// The Rust target a worker binary is built for, such as `x86_64-unknown-linux-gnu`.
    WorkerTarget,
    /// The agent's own ID for a conversation, which is what resumes it.
    AgentSessionId,
    /// Chosen by a client, so that retrying a prompt can't record it twice.
    ClientMessageId,
    /// Chosen by whoever has the file first, as a UUID: a client before it adds the file, so it can show it at once
    /// and adding it again can't add it twice, or the worker for an image a tool sent back. Unique within its session.
    AttachmentId,
    /// Chosen by a client, so that retrying a request that creates something can't create it twice.
    ClientRequestId,
    /// The agent's own ID for a message it's sending.
    MessageId,
    /// The agent's own ID for one use of a tool.
    ToolUseId,
    /// The agent's own ID for work it runs in the background.
    TaskId,
    /// A wait on a workspace's pull request, chosen by the hub. The MCP task its agent follows it by has the same ID.
    WaitId,
    /// A commit, by the full object ID git gives it. It holds any string, as every ID here does, so it's the worker
    /// reading it that refuses one that isn't a full object ID.
    CommitId,
    /// A branch, by its name without `refs/heads/`, such as `euston`, as git spells it.
    BranchName,
    /// A remote of a repository, such as `origin`, as git's config names it.
    RemoteName,
    /// The hub's ID for one of a workspace's other worktrees, as a UUID, since a path names a new one once the last
    /// there is removed.
    ChildWorktreeId,
    /// A worker's own name for something it asks its hub to make, as a UUID or the agent's ID for the tool call that
    /// asked, so a hub that hears the ask again, as each new hub does what the last didn't answer, makes it once.
    /// Unique within the session that asked, as a tool use's ID is.
    AskKey,
);

impl ProviderName {
    /// What a hub calls the provider that runs workers on its own machine.
    pub const LOCAL: &'static str = "local";

    pub fn is_local(&self) -> bool {
        self.as_str() == Self::LOCAL
    }
}
