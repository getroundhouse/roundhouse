//! The forms a client of version 4 reads what became observations in, as fields of records, which a hub that speaks
//! version 4 too fills on every record it sends, whatever the version its connection chose. A fake hub fills each with
//! [`OLD_FORM`]'s word, or a value of its own, that no observation of its world says, so a client of this version that
//! read one would show it, which [`OLD_FORM`] lets a test look for.

use protocol::capability::settings::{Effort, ModelCatalog};
use protocol::client::ClientResponse;
use protocol::model::{
    Account, Branch, BranchName, Branches, CacheTtl, ContextUsage, GithubAccount, GithubApp, GithubUser, GithubUserId,
    Head, HubUpdate, LimitWindow, ModelLabel, ModelList, ModelName, PromptCache, Records, Session, UsageLimit, Wait,
    Workspace,
};

/// What each old form that's text says, so a test can find any shown.
pub const OLD_FORM: &str = "old-form";

/// `records`, with each old form filled.
pub(super) fn records_filled(records: Records) -> Records {
    let mut records = records;
    records.workspaces.iter_mut().for_each(workspace);
    records.sessions.iter_mut().for_each(session);
    records.accounts.iter_mut().for_each(account);
    records.github_accounts.iter_mut().for_each(github_account);
    records.github_apps.iter_mut().for_each(app);
    records.model_lists.iter_mut().for_each(model_list);
    records.hub_update.iter_mut().for_each(hub_update);
    records
}

/// `response`, with each old form in a record it carries filled.
#[expect(clippy::wildcard_enum_match_arm, reason = "picks out the replies that carry a record with an old form")]
pub(super) fn response_filled(response: ClientResponse) -> ClientResponse {
    let mut response = response;
    match &mut response {
        ClientResponse::Workspace(record) => workspace(record),
        ClientResponse::Workspaces(listed) => listed.workspaces.iter_mut().for_each(workspace),
        ClientResponse::Session(record) => session(record),
        ClientResponse::Sessions(listed) => listed.sessions.iter_mut().for_each(session),
        ClientResponse::ModelList(record) => model_list(record),
        ClientResponse::Records(listed) => *listed = records_filled(std::mem::take(listed)),
        _ => {}
    }
    response
}

fn workspace(workspace: &mut Workspace) {
    let name = BranchName::from(OLD_FORM);
    let had = Branch { name: name.clone(), tip: OLD_FORM.into(), upstream: None };
    workspace.branches = Some(Branches { head: Head::Branch { name }, had: vec![had] });
    workspace.worker_build = Some(OLD_FORM.to_string());
}

fn session(session: &mut Session) {
    let parts = Vec::new();
    session.context =
        Some(ContextUsage { used_tokens: 424_242, max_tokens: 500_000, model_tokens: None, max_set_by: None, parts });
    let model = Some(ModelName::named(OLD_FORM));
    session.prompt_cache =
        Some(PromptCache { used_at_ms: super::NOW_MS, ttl: CacheTtl::OneHour, model, tokens: 424_242 });
    for wait in session.waits.iter_mut().flatten().chain(session.wait.as_mut()) {
        open_wait(wait);
    }
}

fn open_wait(wait: &mut Wait) {
    if wait.ended.is_some() {
        return;
    }
    if let Some(watch) = wait.watch.as_mut() {
        watch.read_error = Some(OLD_FORM.to_string());
    }
}

fn account(account: &mut Account) {
    let said = || Some(OLD_FORM.to_string());
    (account.model_provider, account.plan, account.login, account.email) = (said(), said(), said(), said());
    let limit = UsageLimit {
        window: LimitWindow::ModelSevenDays,
        model: Some(ModelLabel::new(OLD_FORM)),
        used_percent: 42,
        resets_at_ms: None,
        heard_at_ms: None,
    };
    account.limits = vec![limit];
}

fn github_account(account: &mut GithubAccount) {
    account.user.name = OLD_FORM.into();
    if let Some(installation) = account.installation.as_mut() {
        installation.on.name = OLD_FORM.into();
    }
}

fn app(app: &mut GithubApp) {
    (app.slug, app.name) = (OLD_FORM.into(), OLD_FORM.to_string());
    app.owner.name = OLD_FORM.into();
    app.bot = Some(GithubUser { id: GithubUserId::new(1), name: OLD_FORM.into() });
}

fn model_list(list: &mut ModelList) {
    let offered = vec![protocol::fixtures::offer(OLD_FORM, OLD_FORM, &[Effort::Low], Effort::Low)];
    list.catalog = Some(ModelCatalog { offered, default_model: None });
}

fn hub_update(update: &mut HubUpdate) {
    update.version = Some(OLD_FORM.to_string());
}
