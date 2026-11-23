//! Moving the revision of a record that holds the old form of an observation, as the observation changes, so a client
//! of a version from before it is sent the record again, as `crate::hub::observations::OldForm` says.

use protocol::model::{Record, RecordKey};

use super::{Store, StoreError};

impl Store {
    /// Moves the revision of the record `key`, which holds the old form of an observation that has changed, and returns
    /// the record. `None` for one that's gone, and for one of a kind that holds no observation's old form.
    pub(crate) fn old_form_moved(&self, key: &RecordKey) -> Result<Option<Record>, StoreError> {
        match key {
            RecordKey::Account { id } => Ok(self.account_moved(id)?.map(Record::Account)),
            RecordKey::Repo { .. }
            | RecordKey::Workspace { .. }
            | RecordKey::Session { .. }
            | RecordKey::Comment { .. }
            | RecordKey::HubSettings
            | RecordKey::NewWorkspaceDraft
            | RecordKey::Subagent { .. }
            | RecordKey::GithubAccount { .. }
            | RecordKey::GithubSignIn { .. }
            | RecordKey::FoundGithubLogin { .. }
            | RecordKey::GithubApp { .. }
            | RecordKey::GithubReach { .. }
            | RecordKey::Terminal { .. }
            | RecordKey::ModelList(_)
            | RecordKey::HubUpdate
            | RecordKey::Proposal { .. }
            | RecordKey::ReplyHeard { .. }
            | RecordKey::Unknown => Ok(None),
        }
    }
}
