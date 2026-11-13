//! Records the hub owns and serves.

mod account;
mod agent;
mod branches;
mod cache;
mod compaction_line;
mod context;
mod counters;
mod draft;
mod effort;
mod files;
mod git;
mod hub_settings;
mod model_name;
mod permission_mode;
mod prompt_line;
mod pull_request;
mod queue;
mod record;
mod session;
mod settings;
mod subagent;
mod title;
mod wait;
mod worktrees;

pub use account::{
    Account, AgentAccount, AgentLogin, HeldLogin, LimitWindow, LoginToken, ModelLabel, SessionAccount, UsageLimit,
};
pub use agent::{AgentDescription, AgentFeature, LoginDescription, ModelChoice, NotOffered};
pub use branches::{Branch, Branches, Head, Upstream};
pub use cache::{CACHE_WORTH_TELLING_TOKENS, CacheRequest, CacheState, CacheTtl, PromptCache};
pub use compaction_line::CompactionLine;
pub use context::{ContextPart, ContextPartKind, ContextUsage, MaxSetBy};
pub use counters::{Place, Rev, RunId, Seq};
pub use draft::{
    AnswerDraft, AnswerSoFar, CommentDraft, Draft, DraftToKeep, NewWorkspaceDraft, NewWorkspaceDraftToKeep,
};
pub use effort::Effort;
pub use files::{FileContents, FileOpened, FilePage, FilePages, OpenedText, PageLine, WorktreeFiles};
pub use git::{GitDiff, GitFile, GitStatus};
pub use hub_settings::{HubSettings, HubSettingsChange};
pub use model_name::{ModelName, ModelNameError};
pub use permission_mode::PermissionMode;
pub use prompt_line::{PromptLine, SettledAt, Settlement};
pub use pull_request::{
    Checks, LeftQueue, Local, MergeState, PrState, PullRequest, PullRequestFound, QueueState, Review, Standing,
};
pub use queue::QueuedPrompt;
pub use record::{Record, RecordKey, Records};
pub use session::{AgentKind, OpenApproval, Session, SessionStatus};
pub use settings::{SessionSettings, SessionSettingsChange};
pub use subagent::{Call, CallResult, RecentCall, RecentWork, Said, Subagent, SubagentStatus};
pub use title::{Title, TitleError, TitleSource};
pub use wait::{Wait, WaitEnd, WaitEnded, WaitUntil};
pub use worktrees::{CHILDREN, ChildStatus, ChildWorktree, MadeBy, WorkName, WorkNameError};

mod ids;
pub use ids::{
    AccountId, AgentSessionId, ApprovalId, AskKey, AttachmentId, BranchName, ChildWorktreeId, ClientMessageId,
    ClientRequestId, CommentId, CommitId, HubId, InstanceId, MessageId, ProviderName, RemoteName, RepoId, SessionId,
    TaskId, ToolUseId, WaitId, WorkerTarget, WorkspaceId,
};
mod repo;
pub use repo::Repo;
mod comment;
pub use comment::{DiffComment, DiffSide};
mod workspace;
pub use workspace::{PORTS_PER_WORKSPACE, SetupStatus, Workspace, WorkspaceStatus};
