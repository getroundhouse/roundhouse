use crate::model::{
    Account, DiffComment, FoundGithubLogin, GithubAccount, GithubApp, GithubReach, GithubSignIn, HubSettings,
    HubUpdate, ModelList, NewWorkspaceDraft, Proposal, Record, RecordKey, ReplyHeard, Repo, Rev, Session, Subagent,
    Terminal, Workspace,
};
use serde::{Deserialize, Serialize};

/// A record changed. Carries the whole record and its revision, so a client only ever replaces its
/// copy with a newer one.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "notice", rename_all = "snake_case")]
pub enum HubNotice {
    Repo(Repo),
    Workspace(Workspace),
    Session(Session),
    Comment(DiffComment),
    HubSettings(HubSettings),
    Account(Account),
    NewWorkspaceDraft(NewWorkspaceDraft),
    /// A subagent's record changed. A client keeps those of the sessions it's showing, which it lists with
    /// `SessionSubagents`.
    Subagent(Subagent),
    GithubAccount(GithubAccount),
    GithubSignIn(GithubSignIn),
    FoundGithubLogin(FoundGithubLogin),
    GithubApp(GithubApp),
    GithubReach(GithubReach),
    /// A terminal was opened, and a hub that kept its workers learned of it again as it started. One that ended or
    /// was closed is `Removed`.
    Terminal(Terminal),
    /// What an agent offers in one place changed, or the hub began or finished asking it. One the hub let go of is
    /// `Removed`.
    ModelList(ModelList),
    /// A proposal changed, or was made. A client keeps those of the sessions it's showing, which it lists with
    /// `SessionProposals`.
    Proposal(Proposal),
    /// How one of a session's replies said out loud ended changed, or was recorded. A client keeps those of the
    /// sessions it's showing, which it lists with `SessionRepliesHeard`.
    ReplyHeard(ReplyHeard),
    /// What a hub installed as a service found in the update feed, each time it reads it.
    HubUpdate(HubUpdate),
    /// The record is gone, as of `rev`.
    Removed {
        record: RecordKey,
        rev: Rev,
    },
    /// This client fell behind and missed some. List everything again, and the subagents, proposals and replies heard
    /// of each session it's showing, which aren't among everything.
    Resync,
    #[serde(other)]
    Unknown,
}

/// What a notice says of one record, whatever its kind, so whoever follows the feed handles every kind at once.
#[derive(Clone, Debug, PartialEq)]
#[expect(
    clippy::large_enum_variant,
    reason = "made from a notice and taken apart straight away, so its size costs nothing"
)]
pub enum RecordNotice {
    /// A copy of the record.
    Copy(Record),
    /// The record is gone, as of `rev`.
    Removed { record: RecordKey, rev: Rev },
}

impl RecordNotice {
    /// Which record it's of.
    pub fn key(&self) -> RecordKey {
        match self {
            RecordNotice::Copy(record) => record.key(),
            RecordNotice::Removed { record, .. } => record.clone(),
        }
    }

    /// The revision of what it says: of the copy, or of the removal.
    pub fn rev(&self) -> Rev {
        match self {
            RecordNotice::Copy(record) => record.rev(),
            RecordNotice::Removed { rev, .. } => *rev,
        }
    }
}

impl TryFrom<HubNotice> for RecordNotice {
    /// A notice that isn't of one record, `Resync` or `Unknown`, handed back as it came.
    type Error = HubNotice;

    fn try_from(notice: HubNotice) -> Result<Self, HubNotice> {
        let record = match notice {
            HubNotice::Repo(repo) => Record::Repo(repo),
            HubNotice::Workspace(workspace) => Record::Workspace(workspace),
            HubNotice::Session(session) => Record::Session(session),
            HubNotice::Comment(comment) => Record::Comment(comment),
            HubNotice::HubSettings(settings) => Record::HubSettings(settings),
            HubNotice::Account(account) => Record::Account(account),
            HubNotice::NewWorkspaceDraft(draft) => Record::NewWorkspaceDraft(draft),
            HubNotice::Subagent(subagent) => Record::Subagent(subagent),
            HubNotice::GithubAccount(account) => Record::GithubAccount(account),
            HubNotice::GithubSignIn(sign_in) => Record::GithubSignIn(sign_in),
            HubNotice::FoundGithubLogin(found) => Record::FoundGithubLogin(found),
            HubNotice::GithubApp(app) => Record::GithubApp(app),
            HubNotice::GithubReach(reach) => Record::GithubReach(reach),
            HubNotice::Terminal(terminal) => Record::Terminal(terminal),
            HubNotice::ModelList(list) => Record::ModelList(list),
            HubNotice::Proposal(proposal) => Record::Proposal(proposal),
            HubNotice::ReplyHeard(heard) => Record::ReplyHeard(heard),
            HubNotice::HubUpdate(update) => Record::HubUpdate(update),
            HubNotice::Removed { record, rev } => return Ok(RecordNotice::Removed { record, rev }),
            HubNotice::Resync | HubNotice::Unknown => return Err(notice),
        };
        Ok(RecordNotice::Copy(record))
    }
}

impl From<Record> for HubNotice {
    fn from(record: Record) -> Self {
        match record {
            Record::Repo(repo) => HubNotice::Repo(repo),
            Record::Workspace(workspace) => HubNotice::Workspace(workspace),
            Record::Session(session) => HubNotice::Session(session),
            Record::Comment(comment) => HubNotice::Comment(comment),
            Record::HubSettings(settings) => HubNotice::HubSettings(settings),
            Record::Account(account) => HubNotice::Account(account),
            Record::NewWorkspaceDraft(draft) => HubNotice::NewWorkspaceDraft(draft),
            Record::Subagent(subagent) => HubNotice::Subagent(subagent),
            Record::GithubAccount(account) => HubNotice::GithubAccount(account),
            Record::GithubSignIn(sign_in) => HubNotice::GithubSignIn(sign_in),
            Record::FoundGithubLogin(found) => HubNotice::FoundGithubLogin(found),
            Record::GithubApp(app) => HubNotice::GithubApp(app),
            Record::GithubReach(reach) => HubNotice::GithubReach(reach),
            Record::Terminal(terminal) => HubNotice::Terminal(terminal),
            Record::ModelList(list) => HubNotice::ModelList(list),
            Record::Proposal(proposal) => HubNotice::Proposal(proposal),
            Record::ReplyHeard(heard) => HubNotice::ReplyHeard(heard),
            Record::HubUpdate(update) => HubNotice::HubUpdate(update),
        }
    }
}

impl From<RecordNotice> for HubNotice {
    fn from(notice: RecordNotice) -> Self {
        match notice {
            RecordNotice::Copy(record) => record.into(),
            RecordNotice::Removed { record, rev } => HubNotice::Removed { record, rev },
        }
    }
}
