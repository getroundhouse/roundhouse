//! The rule's rows for what the hub keeps for every workspace: its settings, the accounts agents run under, the
//! new workspace's draft, and its tool servers.

use protocol::model::{
    HubSettingsChange, NewWorkspaceDraftToKeep, Reserve, Rev, SentCredential, ToolServer, ToolServerAuth,
    ToolServerName, ToolServerUrl, ToolSignInStatus,
};

use super::super::{AddingToolServer, EditingToolServer, KnownBy, Store};
use super::rule::Said;
use crate::login_files::LoginFile;

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

/// The account `ada`, which agents ran under.
pub(super) fn ada(store: &Store) {
    store.account_made(&"ada".into()).unwrap();
}

pub(super) fn account(store: &Store) -> Rev {
    store.account(&"ada".into()).unwrap().unwrap().rev
}

pub(super) fn reserve(store: &Store) -> Said {
    Said::Changed(store.set_reserve(&"ada".into(), Reserve::new(20)).unwrap().is_some())
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

/// The tool server `t`.
pub(super) fn tool_server(store: &Store) {
    let ToolServer { id, name, url, auth, .. } = protocol::fixtures::tool_server("t");
    let adding = AddingToolServer { id: &id, name: &name, url: &url, auth: &auth, secret: None };
    store.add_tool_server(&adding).unwrap();
}

pub(super) fn edit_name(store: &Store) -> Said {
    let name = ToolServerName::stored("Sentry".into());
    let editing = EditingToolServer { name: Some(&name), ..EditingToolServer::default() };
    Said::Changed(store.edit_tool_server(&"t".into(), &editing).unwrap().is_some())
}

/// And where it is and what it's known by are, all at once.
pub(super) fn edit(store: &Store) -> Said {
    let (name, url) = (ToolServerName::stored("Sentry".into()), ToolServerUrl::stored("https://example.com".into()));
    let (auth, file) = (ToolServerAuth::Neither, LoginFile::parse("0b9c7d1e-5a52-4a0e-9d7c-0f6e2f6b8f11").unwrap());
    let known_by = Some(KnownBy { auth: &auth, secret: Some(&file) });
    let editing = EditingToolServer { name: Some(&name), url: Some(&url), known_by };
    Said::Changed(store.edit_tool_server(&"t".into(), &editing).unwrap().is_some())
}

pub(super) fn needs_sign_in(store: &Store) -> Said {
    let auth = ToolServerAuth::SignIn { status: ToolSignInStatus::NeedsSignIn };
    Said::Changed(store.set_tool_server_auth(&"t".into(), &auth).unwrap().is_some())
}

pub(super) fn tool_server_rev(store: &Store) -> Rev {
    store.tool_server(&"t".into()).unwrap().unwrap().0.rev
}
