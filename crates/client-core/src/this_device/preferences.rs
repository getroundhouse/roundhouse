//! What an app remembers from how it's used, which nobody chooses in Settings, so it opens as it was left.

use std::collections::HashMap;
use std::path::Path;

use protocol::model::WorkspaceId;
use protocol::pairing::Fingerprint;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::saving::{NotKept, Saver, read_json, write_json};

const FILE: &str = "preferences.json";

/// What an app remembers from how it's used, which it saves as it changes: the workspace it was left on, on each hub it
/// shows. A hub is the paired one a fingerprint names, or this machine's, with none.
pub struct Preferences {
    kept: Kept,
    saver: Option<Saver<Kept>>,
}

/// What's saved.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Kept {
    last_workspaces: LastWorkspaces,
    /// What a newer build saved that this one doesn't know, kept so that saving here doesn't lose it.
    #[serde(flatten)]
    others: Map<String, Value>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct LastWorkspaces {
    this_machine: Option<WorkspaceId>,
    paired: HashMap<Fingerprint, WorkspaceId>,
}

impl LastWorkspaces {
    fn on(&self, hub: Option<&Fingerprint>) -> Option<&WorkspaceId> {
        match hub {
            Some(hub) => self.paired.get(hub),
            None => self.this_machine.as_ref(),
        }
    }

    fn set(&mut self, hub: Option<&Fingerprint>, workspace: Option<WorkspaceId>) {
        match (hub, workspace) {
            (None, workspace) => self.this_machine = workspace,
            (Some(hub), Some(workspace)) => drop(self.paired.insert(hub.clone(), workspace)),
            (Some(hub), None) => drop(self.paired.remove(hub)),
        }
    }
}

impl Preferences {
    /// Keeps them in `data_dir`, going on from what's saved there. What can't be read counts as nothing saved, and goes
    /// to `failed`, as what can't be saved does, on the saver's thread.
    pub fn in_dir(data_dir: &Path, failed: impl Fn(NotKept) + Send + 'static) -> Self {
        let path = data_dir.join(FILE);
        let kept = read_json(&path).unwrap_or_else(|e| {
            failed(e);
            None
        });
        let saver = Saver::start(move |kept| write_json(&path, kept), failed);
        Self { kept: kept.unwrap_or_default(), saver: Some(saver) }
    }

    /// Keeps them only until the app quits, such as for a picture of the app.
    pub fn in_memory() -> Self {
        Self { kept: Kept::default(), saver: None }
    }

    /// The workspace to open on: the one the app was left on, on `hub`, if `can_show` says the app can still show it.
    pub fn workspace_to_open(
        &self,
        hub: Option<&Fingerprint>,
        can_show: impl FnOnce(&WorkspaceId) -> bool,
    ) -> Option<&WorkspaceId> {
        self.kept.last_workspaces.on(hub).filter(|id| can_show(id))
    }

    /// Keeps `workspace` as the one the app is on, on `hub`, or with none, forgets the one it was.
    pub fn left_on(&mut self, hub: Option<&Fingerprint>, workspace: Option<&WorkspaceId>) {
        if self.kept.last_workspaces.on(hub) != workspace {
            self.kept.last_workspaces.set(hub, workspace.cloned());
            self.save();
        }
    }

    fn save(&self) {
        if let Some(saver) = &self.saver {
            saver.save(self.kept.clone());
        }
    }

    /// Hears once every change so far is saved, for an app to wait for as it quits.
    pub fn saved(&self) -> flume::Receiver<()> {
        match &self.saver {
            Some(saver) => saver.saved(),
            // The sender has gone, so there's nothing to wait for.
            None => flume::bounded(0).1,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    const TIMEOUT: Duration = Duration::from_secs(5);

    /// Those kept in `dir`, where nothing can fail.
    fn kept_in(dir: &Path) -> Preferences {
        Preferences::in_dir(dir, |failure| panic!("{failure}"))
    }

    fn paired_hub() -> Fingerprint {
        Fingerprint::of_sha256([1; 32])
    }

    #[test]
    fn each_hub_opens_on_its_own_once_the_app_opens_again_while_it_can_be_shown() {
        let (dir, paired) = (tempfile::tempdir().unwrap(), paired_hub());
        let (here, there) = (WorkspaceId::from("here"), WorkspaceId::from("there"));
        let mut preferences = kept_in(dir.path());
        assert_eq!(preferences.workspace_to_open(None, |_| true), None, "never left on one");

        preferences.left_on(None, Some(&"before".into()));
        preferences.left_on(Some(&paired), Some(&there));
        preferences.left_on(None, Some(&here));
        preferences.saved().recv_timeout(TIMEOUT).unwrap();

        let again = kept_in(dir.path());
        assert_eq!(again.workspace_to_open(None, |_| true), Some(&here));
        assert_eq!(again.workspace_to_open(Some(&paired), |_| true), Some(&there));
        assert_eq!(
            again.workspace_to_open(Some(&Fingerprint::of_sha256([2; 32])), |_| true),
            None,
            "a hub never shown"
        );
        assert_eq!(again.workspace_to_open(None, |id| *id != here), None, "one that can't be shown any more");
    }

    #[test]
    fn one_forgotten_isnt_opened_again_and_the_others_are() {
        let (dir, paired) = (tempfile::tempdir().unwrap(), paired_hub());
        let mut preferences = kept_in(dir.path());
        preferences.left_on(None, Some(&"here".into()));
        preferences.left_on(Some(&paired), Some(&"there".into()));

        preferences.left_on(Some(&paired), None);
        preferences.saved().recv_timeout(TIMEOUT).unwrap();

        let again = kept_in(dir.path());
        assert_eq!(again.workspace_to_open(Some(&paired), |_| true), None);
        assert_eq!(again.workspace_to_open(None, |_| true), Some(&"here".into()));
    }

    #[test]
    fn whats_saved_that_cant_be_read_is_said_and_counts_as_nothing_saved() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(FILE), "{ not json").unwrap();
        let (said, failed) = flume::unbounded();

        let preferences = Preferences::in_dir(dir.path(), move |failure| drop(said.send(failure)));

        assert!(failed.try_recv().is_ok());
        assert_eq!(preferences.workspace_to_open(None, |_| true), None);
    }

    #[test]
    fn saving_keeps_what_a_newer_build_saved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        std::fs::write(&path, r#"{"last_workspaces": {"this_machine": "here"}, "from_later": [1, 2]}"#).unwrap();

        let mut preferences = kept_in(dir.path());
        preferences.left_on(None, Some(&"elsewhere".into()));
        preferences.saved().recv_timeout(TIMEOUT).unwrap();

        let saved: Value = read_json(&path).unwrap().unwrap();
        let last_workspaces = serde_json::json!({ "this_machine": "elsewhere", "paired": {} });
        assert_eq!(saved, serde_json::json!({ "last_workspaces": last_workspaces, "from_later": [1, 2] }));
    }

    #[test]
    fn those_kept_in_memory_are_kept_while_the_app_is_open_with_nothing_to_wait_for() {
        let mut preferences = Preferences::in_memory();

        preferences.left_on(None, Some(&"here".into()));

        assert_eq!(preferences.workspace_to_open(None, |_| true), Some(&"here".into()));
        assert_eq!(preferences.saved().recv_timeout(TIMEOUT), Err(flume::RecvTimeoutError::Disconnected));
    }
}
