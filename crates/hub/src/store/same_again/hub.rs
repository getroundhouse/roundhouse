//! The rule's rows for what the hub keeps for every workspace: its settings, the accounts agents run under, and the
//! new workspace's draft.

use protocol::model::{HubSettingsChange, LimitWindow, NewWorkspaceDraftToKeep, Rev, SentCredential, UsageLimit};

use super::super::Store;
use super::rule::Said;

pub(super) fn settings(store: &Store) -> Rev {
    store.hub_settings().unwrap().rev
}

pub(super) fn awake(store: &Store) -> Said {
    let off = HubSettingsChange { keep_awake: Some(false), ..HubSettingsChange::default() };
    Said::record(&store.change_hub_settings(&off).unwrap())
}

pub(super) fn allow(store: &Store) -> Said {
    let allowing = HubSettingsChange { allow_asked_in_full_access: Some(true), ..HubSettingsChange::default() };
    Said::record(&store.change_hub_settings(&allowing).unwrap())
}

pub(super) fn sent(store: &Store) -> Said {
    let sent = SentCredential::NoAccount { reason: "Connect one".into() };
    Said::Changed(store.record_github_sent(&sent).unwrap().is_some())
}

pub(super) fn no_default(store: &Store) -> Said {
    Said::record(&store.set_github_default(None).unwrap())
}

pub(super) fn providers(store: &Store) -> Said {
    Said::Changed(store.record_github_providers(&["local".into()], &["vms".into()]).unwrap().is_some())
}

pub(super) fn account(store: &Store) -> Rev {
    store.account(&"ada".into()).unwrap().unwrap().rev
}

pub(super) fn limits(store: &Store) -> Said {
    let limit = UsageLimit {
        window: LimitWindow::SevenDays,
        model: None,
        used_percent: 4,
        resets_at_ms: Some(9_000),
        heard_at_ms: None,
    };
    Said::Changed(store.hear_limits(&"ada".into(), &[limit], false, 100).unwrap().is_some())
}

pub(super) fn new_draft_rev(store: &Store) -> Rev {
    store.new_workspace_draft().unwrap().rev
}

pub(super) fn new_draft(store: &Store) -> Said {
    let typed = NewWorkspaceDraftToKeep {
        repo_id: Some("r".into()),
        text: "fix the login".into(),
        ..NewWorkspaceDraftToKeep::default()
    };
    Said::Changed(store.set_new_workspace_draft(&typed).unwrap().1)
}
