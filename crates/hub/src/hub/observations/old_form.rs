//! The forms a client of a version from before an observation reads it in, as a field of a record the hub still owns,
//! which no longer keeps it, so the hub fills it from the observation as the record leaves the hub: on the feed, as
//! `Hub::notify` tells it, in a list of every record, and in a reply. The record's revision moves as the observation
//! changes, as [`Hub::old_form_moved`] does, and is read back from the store, so what's sent and its revision belong
//! together, as for any change to a record. The field is filled from what's in memory once that revision is taken, so
//! the copy at a record's newest revision holds what the observation says now. Every client is sent the field filled,
//! and one of a version that reads the observation itself doesn't read it.

use protocol::client::ClientResponse;
use protocol::model::{
    Account, GithubAccount, GithubApp, ModelList, Record, RecordKey, Records, Session, UsageLimit, Wait,
};
use tracing::warn;

use super::github_kinds::{GithubAppBot, GithubAppSaid, GithubUsername, InstalledOn, RepositoryReach};
use super::kept::Observations;
use super::kinds::{
    AccountIdentity, AccountLimits, Kind, ModelCatalog, SessionCache, SessionContext, SessionKey, SessionTodos,
    WaitKey, WaitRead,
};
use super::tables::Tables;
use crate::hub::Hub;

/// A record, or what holds records, with a field a client of a version from before an observation reads it in.
pub(crate) trait OldForm {
    /// Fills each such field from what `tables` keeps.
    fn fill(&mut self, tables: &mut Tables);
}

/// What GitHub calls the account's user, on its record. While it isn't known, as before the hub has first asked GitHub
/// since it stopped keeping it in the database, it's as the store reads it, the user's number, as `#<number>`, since
/// the field isn't optional.
impl OldForm for GithubAccount {
    fn fill(&mut self, tables: &mut Tables) {
        if let Some(username) = GithubUsername::table(tables).value(&self.id) {
            self.user.name = username.clone();
        }
        // What an app's installation is on, likewise, named by its number while it isn't known.
        if let Some(installation) = &mut self.installation
            && let Some(on) = InstalledOn::table(tables).value(&self.id)
        {
            installation.on.name = on.clone();
        }
    }
}

/// What GitHub says of the app, and its bot, on its record. While what GitHub says isn't known, as before the hub has
/// first asked GitHub as the app since it stopped keeping it in the database, it's as the store reads it: the app's
/// number, as `#<number>`, for its slug and its name, an owner of no number, `#0`, and no bot.
impl OldForm for GithubApp {
    fn fill(&mut self, tables: &mut Tables) {
        if let Some(said) = GithubAppSaid::table(tables).value(&self.id) {
            let protocol::model::GithubAppSaid { slug, name, owner } = said.clone();
            (self.slug, self.name, self.owner) = (slug, name, owner);
        }
        if let Some(bot) = GithubAppBot::table(tables).value(&self.id) {
            self.bot = Some(bot.clone());
        }
    }
}

/// Who the account is, and what's left of its limits, on its record, which says nothing of when each was heard.
impl OldForm for Account {
    fn fill(&mut self, tables: &mut Tables) {
        let identity = AccountIdentity::table(tables).value(&self.id).cloned().unwrap_or_default();
        let protocol::model::AccountIdentity { model_provider, plan, login, email } = identity;
        (self.model_provider, self.plan, self.login, self.email) = (model_provider, plan, login, email);
        let limits = AccountLimits::table(tables).value(&self.id).map_or_default(Vec::as_slice);
        self.limits = limits.iter().map(|limit| UsageLimit { heard_at_ms: None, ..limit.clone() }).collect();
    }
}

/// What its agent offers, on a list of models' record, which the hub keeps in memory with how its lookup stands.
impl OldForm for ModelList {
    fn fill(&mut self, tables: &mut Tables) {
        self.catalog = ModelCatalog::table(tables).value(&self.key).cloned();
    }
}

/// How full its agent's context is, its to-do list, and what the API's cache holds of its conversation, on a session's
/// record, each `None` while it isn't known.
impl OldForm for Session {
    fn fill(&mut self, tables: &mut Tables) {
        let of = SessionKey { workspace_id: self.workspace_id.clone(), session_id: self.id.clone() };
        self.context = SessionContext::table(tables).value(&of).cloned();
        self.todos = SessionTodos::table(tables).value(&of).cloned();
        self.prompt_cache = SessionCache::table(tables).value(&of).and_then(|cache| cache.held.clone());
        for wait in self.waits.iter_mut().flatten().chain(self.wait.as_mut()) {
            wait.fill(tables);
        }
    }
}

/// What an open wait last read, in its watch, as its record keeps it while that isn't known. An ended wait's record
/// keeps its last read, frozen as it ended.
impl OldForm for Wait {
    fn fill(&mut self, tables: &mut Tables) {
        if self.ended.is_some() {
            return;
        }
        let of = WaitKey {
            workspace_id: self.workspace_id.clone(),
            session_id: self.session_id.clone(),
            wait: self.id.clone(),
        };
        let (Some(watch), Some(read)) = (self.watch.as_mut(), WaitRead::table(tables).value(&of)) else { return };
        **watch = watch.recorded().with_read(read.clone());
    }
}

impl OldForm for Record {
    fn fill(&mut self, tables: &mut Tables) {
        match self {
            Record::Account(account) => account.fill(tables),
            Record::ModelList(list) => list.fill(tables),
            Record::Session(session) => session.fill(tables),
            Record::GithubAccount(account) => account.fill(tables),
            Record::GithubApp(app) => app.fill(tables),
            // Hold no observation's old form.
            Record::Repo(_)
            | Record::Workspace(_)
            | Record::Comment(_)
            | Record::HubSettings(_)
            | Record::NewWorkspaceDraft(_)
            | Record::Subagent(_)
            | Record::GithubSignIn(_)
            | Record::FoundGithubLogin(_)
            | Record::GithubReach(_)
            | Record::Terminal(_)
            | Record::HubUpdate(_)
            | Record::Proposal(_)
            | Record::ReplyHeard(_) => {}
        }
    }
}

/// Each record of a kind that holds an observation's old form, and each record made from an observation, which the
/// store keeps none of.
impl OldForm for Records {
    fn fill(&mut self, tables: &mut Tables) {
        for account in &mut self.accounts {
            account.fill(tables);
        }
        for list in &mut self.model_lists {
            list.fill(tables);
        }
        for session in &mut self.sessions {
            session.fill(tables);
        }
        for account in &mut self.github_accounts {
            account.fill(tables);
        }
        for app in &mut self.github_apps {
            app.fill(tables);
        }
        let reaches = made_from::<RepositoryReach>(tables).into_iter().filter_map(|record| {
            let Record::GithubReach(reach) = record else { return None };
            Some(reach)
        });
        self.github_reach = reaches.collect();
        self.github_reach.sort_by(|a, b| a.repo.cmp(&b.repo));
    }
}

/// The replies that carry a record holding an observation's old form: an account's, a GitHub account's, an app of your own's, a list of models', and a session's,
/// alone, in a list of a workspace's, or as a new workspace's first. Every reply is named, so a new one that carries such a record is
/// filled. Those that carry every record are filled as they're listed, and the hub reads a `SessionAccount` from an
/// account's record it fills.
impl OldForm for ClientResponse {
    #[expect(clippy::too_many_lines, reason = "a flat match over every reply, each arm a call or nothing")]
    fn fill(&mut self, tables: &mut Tables) {
        use ClientResponse as C;
        match self {
            C::Account(account) => account.fill(tables),
            C::Session(session) => session.fill(tables),
            C::Sessions(sessions) => {
                for session in &mut sessions.sessions {
                    session.fill(tables);
                }
            }
            C::GithubAccount(account) => account.fill(tables),
            C::GithubApp(app) => app.fill(tables),
            C::ModelList(list) => list.fill(tables),
            C::NewWorkspaceMade(made) => made.session.fill(tables),
            // Hold no record that holds an observation's old form, or are filled as they're listed.
            C::Paired(_)
            | C::Devices(_)
            | C::Pairing(_)
            | C::Device(_)
            | C::Hello(_)
            | C::Ok
            | C::HubStatus(_)
            | C::HubSettings(_)
            | C::NewWorkspaceDraft(_)
            | C::Repo(_)
            | C::Repos(_)
            | C::Records(_)
            | C::Agents(_)
            | C::Workspace(_)
            | C::Workspaces(_)
            | C::GitStatus(_)
            | C::GitDiff(_)
            | C::ChangedFilesPage(_)
            | C::FileDiff(_)
            | C::SetupLog(_)
            | C::PullRequest(_)
            | C::Comment(_)
            | C::Comments(_)
            | C::Events(_)
            | C::Subagents(_)
            | C::Proposals(_)
            | C::RepliesHeard(_)
            | C::References(_)
            | C::Proposal(_)
            | C::ProposalRevisions(_)
            | C::PlacedEvents(_)
            | C::Prompts(_)
            | C::Compactions(_)
            | C::Found(_)
            | C::PromptRecorded(_)
            | C::Attachment(_)
            | C::AttachmentData(_)
            | C::SessionAccount(_)
            | C::GithubSignIn(_)
            | C::GhLogin(_)
            | C::MachineLogins(_)
            | C::GithubAppManifest(_)
            | C::WorktreeFiles(_)
            | C::FileContents(_)
            | C::FileOpened(_)
            | C::FilePages(_)
            | C::FileRange(_)
            | C::Terminal(_)
            | C::TerminalRows(_)
            | C::TerminalBounds(_)
            | C::TerminalText(_)
            | C::TerminalFit(_)
            | C::Subscribed(_) => {}
        }
    }
}

/// Each record made from an observation of kind `K` that `tables` keeps, as [`Kind::old_record`] makes it.
fn made_from<K: Kind>(tables: &mut Tables) -> Vec<Record> {
    let kept = &K::table(tables).kept;
    kept.iter()
        .filter_map(|(of, kept)| K::old_record(of, kept.said.observed(), kept.said.read_at_ms, kept.rev))
        .collect()
}

impl Observations {
    /// Fills each field of `record` that holds an observation's old form, as [`OldForm`] says.
    pub(crate) fn fill(&self, record: &mut impl OldForm) {
        record.fill(&mut self.tables.lock());
    }
}

impl Hub {
    /// Fills each field of `record`, or of each record it holds, that holds an observation's old form, as [`OldForm`]
    /// says, for a reply.
    pub(crate) fn fill_old_forms(&self, record: &mut impl OldForm) {
        self.observations.fill(record);
    }

    /// Moves the revision of `record`, which holds the old form of an observation that has just changed, and tells
    /// clients of version 4 and older of it, filled with what the observation says now, as
    /// [`Hub::notify_old_form`] does. A record that's gone tells nobody anything.
    pub(super) async fn old_form_moved(&self, record: RecordKey) {
        match self.blocking(move |store| store.old_form_moved(&record)).await {
            Ok(Some(moved)) => self.notify_old_form(moved.into()),
            Ok(None) => {}
            Err(e) => warn!("moving the revision of a record that holds an observation's old form: {}", e.message),
        }
    }
}
