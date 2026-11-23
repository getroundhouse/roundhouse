//! Records the hub owns and serves, and its observations of what others own.

mod account;
mod agent;
mod branches;
mod cache;
mod changes;
mod client_device;
mod compaction_line;
mod context;
mod counters;
mod draft;
mod files;
mod git;
mod github_account;
mod hub_settings;
mod hub_update;
mod interaction;
mod level;
mod model_list;
mod model_name;
mod observation;
mod pr_watch;
mod prompt_line;
mod proposal;
mod pull_request;
mod queue;
mod record;
mod references;
mod reply_heard;
mod session;
mod subagent;
mod terminal;
mod title;
mod wait;
mod worktrees;

pub use crate::prompt::PromptFrom;
pub use account::{
    Account, AccountSource, AgentAccount, AgentLogin, HeldLogin, LimitWindow, LoginToken, ModelLabel, Reserve,
    ReserveOutOfRange, SessionAccount, UsageLimit,
};
pub use agent::{AgentDescription, LoginDescription, NotOffered, RunCapabilities};
pub use branches::{Apart, Branch, Branches, Head, Upstream};
pub use cache::{CACHE_WORTH_TELLING_TOKENS, CacheRequest, CacheState, CacheTtl, PromptCache};
pub use changes::{
    ChangeStatus, ChangeSummary, ChangedFile, ChangedFileAt, ChangedFilesPage, FileDiff, FileDiffBody, FileVersion,
    LineCounts, MOST_FILES_LISTED, MOST_PATCH_PAGE_BYTES,
};
pub use client_device::ClientDevice;
pub use compaction_line::CompactionLine;
pub use context::{ContextPart, ContextPartKind, ContextUsage, MaxSetBy};
pub use counters::{Place, Rev, RunId, Seq};
pub use draft::{
    AnswerSoFar, CommentDraft, DecisionDraft, Draft, DraftToKeep, NewWorkspaceDraft, NewWorkspaceDraftToKeep, SoFar,
    UseChoice,
};
pub use files::{
    ByteRange, FileContents, FileOpened, FilePage, FilePages, FileRange, MAX_RANGE_BYTES, OpenedText, PageLine,
    WorktreeFiles,
};
pub use git::{GitDiff, GitFile, GitStatus};
pub use github_account::{
    FoundGithubLogin, GhLogin, GhLoginWorks, GithubAccount, GithubApp, GithubAppId, GithubInstallation,
    GithubInstallationId, GithubLoginKind, GithubReach, GithubRepositoryName, GithubRepositoryNameError, GithubSignIn,
    GithubSignInStatus, GithubToken, GithubUser, GithubUserId, HeldGithubLogin, InstalledApp, MachineLogins, Reach,
    SentCredential, SshLogin, UnusableLogin,
};
pub use hub_settings::{AccountDefault, GithubSettings, HubSettings, HubSettingsChange};
pub use hub_update::{HubUpdate, UpdateAttempt};
pub use interaction::{Interaction, InteractionMode};
pub use level::Level;
pub use model_list::{CliVersion, ModelList, ModelListKey, ModelLookup};
pub use model_name::{ModelName, ModelNameError};
pub use observation::{
    AccountIdentity, IssueSaid, Observation, ObservationKey, Observed, OwnPullRequestDetail, OwnPullRequestSaid,
    OwnSummary, PullRequestDetail, PullRequestSaid, ReadFrom, ReferenceSaid,
};
pub use prompt_line::{PromptLine, SettledAt, Settlement};
pub use proposal::{
    AnswerProblem, Choice, ChosenOption, MAX_HEADER_CHARS, MAX_NOTE_BYTES, MAX_OPTIONS, MAX_PROPOSAL_BYTES,
    MAX_QUESTIONS, MIN_OPTIONS, NOTE_START_CHARS, OptionLetter, OptionPlace, Proposal, ProposalOption, ProposalProblem,
    ProposalQuestion, ProposalRevision, Proposed, Recommendation, Response, ResponseSummary, RevisionNumber,
    RevisionStanding, RevisionSummary, Round, Withdrawal, chosen_in_words,
};
pub(crate) use pull_request::deserialize_looked;
pub use pull_request::{
    CheckCounts, CheckState, Checks, ChecksRollup, History, LeftQueue, Local, MergeState, Mergeability, ObservedCheck,
    OwnPullRequest, Phase, PrState, PullRequest, PullRequestFound, PullRequestLink, PullRequestLinked, QueueState,
    Read, RepositoryUnseen, Review, Stack, Standing, Summary,
};
pub use queue::QueuedPrompt;
pub use record::{Record, RecordKey, Records};
pub use references::{
    About, Issue, IssueNumber, IssueNumberError, IssueState, IssueSummary, REFERENCE_BODY_CHARS, Reference,
    ReferenceAsk, ReferenceLabel, ReferenceRead,
};
pub use reply_heard::{Ending, ReplyHeard, ReplyPlace, SettledBy, SpokenThrough};
pub use session::{AgentKind, OpenApproval, Session, SessionStatus};
pub use subagent::{Lately, LatelyCall, LatelyResult, Said, Subagent, SubagentStatus, TouchedSubagents};
pub use terminal::Terminal;
pub use title::{Title, TitleError, TitleSource};
pub use wait::{EndedWait, Wait, WaitEnd, WaitEnded, WaitUntil, Watching};
pub use worktrees::{CHILDREN, ChildStatus, ChildWorktree, MadeBy, WorkName, WorkNameError};

mod ids;
pub(crate) use ids::ids;
mod tokens;
pub use ids::{
    AccountId, AgentSessionId, ApprovalId, AskKey, AttachmentId, BranchName, ChildWorktreeId, ClientMessageId,
    ClientRequestId, CommentId, CommitId, DeviceId, GithubAccountId, GithubAppSlug, GithubSignInId, GithubUserCode,
    GithubUsername, HubId, InstanceId, MessageId, ProposalId, ProviderName, RemoteName, RepoId, SessionId, SubagentId,
    TaskId, TerminalId, ToolUseId, WaitId, WorkerTarget, WorkspaceId,
};
mod repo;
pub use repo::{Repo, RepoOrigin, RepoUrl, RepoUrlError};
mod comment;
pub use comment::{DiffComment, DiffSide};
mod workspace;
pub use workspace::{PORTS_PER_WORKSPACE, SetupStatus, Workspace, WorkspaceStatus};

pub use pr_watch::{
    GitHubRepository, PullRequestNumber, PullRequestSelection, PullRequestWaitResult, PullRequestWatch,
    PullRequestsSnapshot, SelectionError, WaitBaseline, WaitOutcome,
};
