//! What a test hands a client of this version as the hub's observations, made from records as a client of version 4
//! is sent them, so a test that says what a chat's context holds, or an account's plan, says it where it always has.

use protocol::model::{
    AccountIdentity, ConversationCache, GithubAppSaid, Observation, Observed, OtherBuild, Records, ReleaseOffer,
    Session, Workspace,
};

/// Every one of `records`' observations, those [`observed_of`] makes of them included, as a hub's list of every
/// record carries them.
pub fn with_observations(records: Records) -> Records {
    let mut observations = observed_of(&records);
    observations.extend(records.observations.iter().cloned());
    Records { observations, ..records }
}

/// One observation for each thing `records` say that a hub keeps as one, as a hub of version 5 would send them, such as
/// an account's plan or a chat's context, so a test says each where a record of version 4 holds it, and holds them as
/// a client of this version reads them. Each is as of when the picture is drawn, or current, as its kind is.
pub fn observed_of(records: &Records) -> Vec<Observation> {
    let mut made = Vec::new();
    for account in &records.accounts {
        let identity = AccountIdentity {
            model_provider: account.model_provider.clone(),
            plan: account.plan.clone(),
            login: account.login.clone(),
            email: account.email.clone(),
        };
        if identity != AccountIdentity::default() {
            made.push(Observation::AccountIdentity {
                account: account.id.clone(),
                identity: said(identity),
                rev: MADE_AT,
            });
        }
        if !account.limits.is_empty() {
            let limits = said(account.limits.clone());
            made.push(Observation::AccountLimits { account: account.id.clone(), limits, rev: MADE_AT });
        }
    }
    for account in &records.github_accounts {
        let username = current(account.user.name.clone());
        made.push(Observation::GithubUsername { account: account.id.clone(), username, rev: MADE_AT });
        if let Some(installed) = &account.installation {
            let on = current(installed.on.name.clone());
            made.push(Observation::InstalledOn { account: account.id.clone(), on, rev: MADE_AT });
        }
    }
    for app in &records.github_apps {
        let app_said = GithubAppSaid { slug: app.slug.clone(), name: app.name.clone(), owner: app.owner.clone() };
        made.push(Observation::GithubAppSaid { app: app.id, said: current(app_said), rev: MADE_AT });
        if let Some(bot) = &app.bot {
            made.push(Observation::GithubAppBot { app: app.id, bot: current(bot.clone()), rev: MADE_AT });
        }
    }
    for reach in &records.github_reach {
        made.push(Observation::RepositoryReach {
            repo: reach.repo.clone(),
            reach: current(reach.reach.clone()),
            rev: MADE_AT,
        });
    }
    for list in &records.model_lists {
        if let Some(catalog) = &list.catalog {
            made.push(Observation::ModelCatalog {
                list: list.key.clone(),
                catalog: said(catalog.clone()),
                rev: MADE_AT,
            });
        }
    }
    if let Some(version) = records.hub_update.as_ref().and_then(|update| update.version.clone()) {
        made.push(Observation::ReleaseOffered {
            offered: current(ReleaseOffer { version: Some(version) }),
            rev: MADE_AT,
        });
    }
    made.extend(workspaces_made_of(&records.workspaces));
    made.extend(sessions_made_of(&records.sessions));
    made
}

/// Each workspace's branches, and which build its worker is where it's another.
fn workspaces_made_of(workspaces: &[Workspace]) -> Vec<Observation> {
    let mut made = Vec::new();
    for workspace in workspaces {
        let workspace_id = workspace.id.clone();
        if let Some(branches) = &workspace.branches {
            let branches = current(branches.clone());
            made.push(Observation::WorkspaceBranches { workspace_id: workspace_id.clone(), branches, rev: MADE_AT });
        }
        if let Some(build) = &workspace.worker_build {
            let build = current(OtherBuild { said: Some(build.clone()) });
            made.push(Observation::WorkspaceWorkerBuild { workspace_id, build, rev: MADE_AT });
        }
    }
    made
}

/// Each chat's context, to-do list and cache, and what each of its open waits last read.
fn sessions_made_of(sessions: &[Session]) -> Vec<Observation> {
    let mut made = Vec::new();
    for session in sessions {
        let session_id = session.id.clone();
        if let Some(context) = &session.context {
            made.push(Observation::SessionContext {
                session_id: session_id.clone(),
                context: said(context.clone()),
                rev: MADE_AT,
            });
        }
        if let Some(todos) = &session.todos {
            made.push(Observation::SessionTodos {
                session_id: session_id.clone(),
                todos: said(todos.clone()),
                rev: MADE_AT,
            });
        }
        if session.prompt_cache.is_some() || session.context.is_some() {
            let cache = ConversationCache::worked_out(session.prompt_cache.clone(), session.context.as_ref());
            made.push(Observation::SessionCache { session_id: session_id.clone(), cache: said(cache), rev: MADE_AT });
        }
        for wait in session.open_waits() {
            let Some(watch) = &wait.watch else { continue };
            let read = said(watch.read());
            made.push(Observation::WaitRead {
                session_id: session_id.clone(),
                wait: wait.id.clone(),
                read,
                rev: MADE_AT,
            });
        }
    }
    made
}

/// `value`, as kept up to date, as what the hub watches or asks for again is.
fn current<T>(value: T) -> Observed<T> {
    Observed::Current { value }
}

/// `value`, as of the moment every picture is drawn at, as what only an agent says is: never current.
fn said<T>(value: T) -> Observed<T> {
    Observed::AsOf { value, as_of_ms: AS_OF_MS }
}

/// When what only an agent says was said, which is when every picture is drawn at.
const AS_OF_MS: i64 = 1_793_545_200_000;

/// The revision of each, as of the record that makes it.
const MADE_AT: protocol::model::Rev = protocol::model::Rev::new(1);
