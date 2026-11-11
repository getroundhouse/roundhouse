//! What a client holds of one hub's records: its repositories, workspaces, sessions and comments, its settings, and
//! the accounts its agents run under.
//!
//! No IO. Lists, feed notices and replies can arrive in any order, and each record keeps whichever copy
//! has the newer revision, so the result is the same however they interleave. A hub that doesn't keep
//! revisions sends 0, and then the copy that arrives last wins. One ordered connection keeps that
//! right for lists and notices, though a reply can still overtake a notice.

use std::collections::HashMap;

use protocol::client::HubNotice;
use protocol::model::{
    Account, AccountId, DiffComment, HubSettings, RecordKey, Records, Repo, Rev, Session, Workspace, WorkspaceId,
};

#[cfg(test)]
mod laws;

/// One record of any kind.
#[derive(Clone, Debug, PartialEq)]
pub enum Record {
    Repo(Repo),
    Workspace(Workspace),
    Session(Session),
    Comment(DiffComment),
    HubSettings(HubSettings),
    Account(Account),
}

impl Record {
    pub fn key(&self) -> RecordKey {
        match self {
            Record::Repo(repo) => RecordKey::Repo { id: repo.id.clone() },
            Record::Workspace(workspace) => RecordKey::Workspace { id: workspace.id.clone() },
            Record::Session(session) => RecordKey::Session { id: session.id.clone() },
            Record::Comment(comment) => RecordKey::Comment { id: comment.id.clone() },
            Record::HubSettings(_) => RecordKey::HubSettings,
            Record::Account(account) => RecordKey::Account { id: account.id.clone() },
        }
    }

    pub fn rev(&self) -> Rev {
        match self {
            Record::Repo(repo) => repo.rev,
            Record::Workspace(workspace) => workspace.rev,
            Record::Session(session) => session.rev,
            Record::Comment(comment) => comment.rev,
            Record::HubSettings(settings) => settings.rev,
            Record::Account(account) => account.rev,
        }
    }
}

/// Whether a copy at `incoming` replaces one at `held`. Without revisions, the later arrival does.
fn supersedes(incoming: Rev, held: Rev) -> bool {
    incoming == Rev::default() || incoming > held
}

#[derive(Debug, Default)]
pub struct HubRecords {
    held: HashMap<RecordKey, Record>,
    /// Records removed since the newest list, and when, so an older copy that arrives late can't bring
    /// one back.
    removed: HashMap<RecordKey, Rev>,
    /// The newest list taken in. Everything up to it is already reflected.
    listed: Rev,
}

impl HubRecords {
    /// Takes in a list of every record. A record missing from it is gone, unless the copy held is
    /// newer than the list. Returns whether anything changed.
    pub fn apply_list(&mut self, list: Records) -> bool {
        let at = list.rev;
        // Older than one already taken in, so everything in it is too.
        if at != Rev::default() && at < self.listed {
            return false;
        }
        let listed = list.repos.into_iter().map(Record::Repo);
        let listed = listed.chain(list.workspaces.into_iter().map(Record::Workspace));
        let listed = listed.chain(list.sessions.into_iter().map(Record::Session));
        let listed = listed.chain(list.comments.into_iter().map(Record::Comment));
        let listed = listed.chain(list.hub_settings.into_iter().map(Record::HubSettings));
        let listed = listed.chain(list.accounts.into_iter().map(Record::Account));
        let mut next: HashMap<RecordKey, Record> = HashMap::new();
        for record in listed {
            let key = record.key();
            if self.removed.get(&key).is_some_and(|&gone| at != Rev::default() && gone > record.rev()) {
                continue;
            }
            next.insert(key, record);
        }
        for (key, held) in &self.held {
            let newer_than_list = at != Rev::default() && held.rev() > at;
            let newer_than_listed = next.get(key).is_none_or(|listed| !supersedes(listed.rev(), held.rev()));
            if newer_than_list && newer_than_listed {
                next.insert(key.clone(), held.clone());
            }
        }
        self.removed.retain(|_, gone| at != Rev::default() && *gone > at);
        self.listed = self.listed.max(at);
        let changed = next != self.held;
        self.held = next;
        changed
    }

    /// Takes in what the feed said. Returns whether anything changed. `Resync` changes nothing here:
    /// it asks for a new list.
    pub fn apply_notice(&mut self, notice: HubNotice) -> bool {
        match notice {
            HubNotice::Repo(repo) => self.apply_record(Record::Repo(repo)),
            HubNotice::Workspace(workspace) => self.apply_record(Record::Workspace(workspace)),
            HubNotice::Session(session) => self.apply_record(Record::Session(session)),
            HubNotice::Comment(comment) => self.apply_record(Record::Comment(comment)),
            HubNotice::HubSettings(settings) => self.apply_record(Record::HubSettings(settings)),
            HubNotice::Account(account) => self.apply_record(Record::Account(account)),
            HubNotice::Removed { record, rev } => self.remove(&record, rev),
            HubNotice::Resync | HubNotice::Unknown => false,
        }
    }

    fn already_listed(&self, rev: Rev) -> bool {
        rev != Rev::default() && rev <= self.listed
    }

    /// Takes in one record, from a notice or a reply. Returns whether it changed anything.
    pub fn apply_record(&mut self, record: Record) -> bool {
        let (key, rev) = (record.key(), record.rev());
        let removed_since = self.removed.get(&key).is_some_and(|&gone| !supersedes(rev, gone));
        let held_is_newer = self.held.get(&key).is_some_and(|held| !supersedes(rev, held.rev()));
        if self.already_listed(rev) || removed_since || held_is_newer || self.held.get(&key) == Some(&record) {
            return false;
        }
        self.held.insert(key, record);
        true
    }

    fn remove(&mut self, key: &RecordKey, rev: Rev) -> bool {
        if self.already_listed(rev) {
            return false;
        }
        let gone = self.removed.entry(key.clone()).or_default();
        *gone = (*gone).max(rev);
        if self.held.get(key).is_some_and(|held| supersedes(rev, held.rev())) {
            self.held.remove(key);
            return true;
        }
        false
    }

    /// By name.
    pub fn repos(&self) -> Vec<Repo> {
        let mut repos: Vec<Repo> =
            self.of_kind(|record| if let Record::Repo(repo) = record { Some(repo.clone()) } else { None });
        repos.sort_by(|a, b| (&a.name, &a.id).cmp(&(&b.name, &b.id)));
        repos
    }

    /// Oldest first, archived ones included.
    pub fn workspaces(&self) -> Vec<Workspace> {
        let mut workspaces: Vec<Workspace> = self
            .of_kind(|record| if let Record::Workspace(workspace) = record { Some(workspace.clone()) } else { None });
        workspaces.sort_by(|a, b| (a.created_at_ms, &a.id).cmp(&(b.created_at_ms, &b.id)));
        workspaces
    }

    /// Each workspace's sessions, oldest first. A workspace with none has no entry.
    pub fn sessions(&self) -> HashMap<WorkspaceId, Vec<Session>> {
        let sessions =
            self.of_kind(|record| if let Record::Session(session) = record { Some(session.clone()) } else { None });
        by_workspace(sessions, |s| (&s.workspace_id, s.created_at_ms, s.id.as_str()))
    }

    /// Each workspace's comments, oldest first. A workspace with none has no entry.
    pub fn comments(&self) -> HashMap<WorkspaceId, Vec<DiffComment>> {
        let comments =
            self.of_kind(|record| if let Record::Comment(comment) = record { Some(comment.clone()) } else { None });
        by_workspace(comments, |c| (&c.workspace_id, c.created_at_ms, c.id.as_str()))
    }

    /// The hub's own settings. `None` until they've arrived, and for good from a hub from before it kept any.
    pub fn hub_settings(&self) -> Option<HubSettings> {
        let Some(Record::HubSettings(settings)) = self.held.get(&RecordKey::HubSettings) else { return None };
        Some(settings.clone())
    }

    /// The record of the account `id`. `None` until it's arrived, and from a hub from before it kept them.
    pub fn account(&self, id: &AccountId) -> Option<Account> {
        let Some(Record::Account(account)) = self.held.get(&RecordKey::Account { id: id.clone() }) else { return None };
        Some(account.clone())
    }

    fn of_kind<T>(&self, pick: impl Fn(&Record) -> Option<T>) -> Vec<T> {
        self.held.values().filter_map(pick).collect()
    }
}

/// Groups records by workspace, each group oldest first. `place` gives a record's workspace, when it was
/// made, and its ID, which settles a tie.
fn by_workspace<T>(
    mut records: Vec<T>,
    place: impl Fn(&T) -> (&WorkspaceId, i64, &str),
) -> HashMap<WorkspaceId, Vec<T>> {
    records.sort_by(|a, b| {
        let ((_, a_at, a_id), (_, b_at, b_id)) = (place(a), place(b));
        (a_at, a_id).cmp(&(b_at, b_id))
    });
    let mut grouped: HashMap<WorkspaceId, Vec<T>> = HashMap::new();
    for record in records {
        let workspace = place(&record).0.clone();
        grouped.entry(workspace).or_default().push(record);
    }
    grouped
}
