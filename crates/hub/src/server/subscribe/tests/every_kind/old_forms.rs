//! What a copy of a record says in each field a client of version 4 reads an observation in, as the round the test made
//! the observation in, which `making` writes into each, or `None` where it says nothing, or is of another kind.

use protocol::model::{Head, Reach, Record};

pub(super) fn branches(record: &Record) -> Option<String> {
    let Record::Workspace(workspace) = record else { return None };
    let Head::Branch { name } = &workspace.branches.as_ref()?.head else { return None };
    Some(name.to_string())
}

pub(super) fn worker_build(record: &Record) -> Option<String> {
    let Record::Workspace(workspace) = record else { return None };
    workspace.worker_build.clone()
}

pub(super) fn reach(record: &Record) -> Option<String> {
    let Record::GithubReach(reach) = record else { return None };
    let Reach::Pushes { username, .. } = &reach.reach else { return None };
    Some(username.to_string())
}

pub(super) fn username(record: &Record) -> Option<String> {
    let Record::GithubAccount(account) = record else { return None };
    Some(account.user.name.to_string())
}

pub(super) fn installed_on(record: &Record) -> Option<String> {
    let Record::GithubAccount(account) = record else { return None };
    Some(account.installation.as_ref()?.on.name.to_string())
}

pub(super) fn app_name(record: &Record) -> Option<String> {
    let Record::GithubApp(app) = record else { return None };
    Some(app.name.clone())
}

pub(super) fn app_bot(record: &Record) -> Option<String> {
    let Record::GithubApp(app) = record else { return None };
    Some(app.bot.as_ref()?.name.to_string())
}

pub(super) fn catalog(record: &Record) -> Option<String> {
    let Record::ModelList(list) = record else { return None };
    Some(list.catalog.as_ref()?.default_model.as_ref()?.as_str().to_string())
}

pub(super) fn plan(record: &Record) -> Option<String> {
    let Record::Account(account) = record else { return None };
    account.plan.clone()
}

pub(super) fn limits(record: &Record) -> Option<String> {
    let Record::Account(account) = record else { return None };
    Some(account.limits.first()?.used_percent.to_string())
}

pub(super) fn context(record: &Record) -> Option<String> {
    let Record::Session(session) = record else { return None };
    Some(session.context.as_ref()?.used_tokens.to_string())
}

pub(super) fn todos(record: &Record) -> Option<String> {
    let Record::Session(session) = record else { return None };
    session.todos.as_ref()?.note.clone()
}

pub(super) fn prompt_cache(record: &Record) -> Option<String> {
    let Record::Session(session) = record else { return None };
    Some(session.prompt_cache.as_ref()?.tokens.to_string())
}

pub(super) fn wait_read(record: &Record) -> Option<String> {
    let Record::Session(session) = record else { return None };
    session.open_waits().first()?.watch.as_ref()?.read_error.clone()
}

pub(super) fn release(record: &Record) -> Option<String> {
    let Record::HubUpdate(update) = record else { return None };
    update.version.clone()
}
