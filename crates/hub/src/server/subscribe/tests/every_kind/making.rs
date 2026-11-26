//! The test's hub, and an observation of each kind made on it, each saying the round it's made in, so a copy made in
//! one round can be told from one made in the other.

use std::sync::Arc;

use protocol::capability::output::TodoList;
use protocol::capability::settings::ModelCatalog;
use protocol::capability::setup::{ServerState, ToolServerState};
use protocol::client::HubNotice;
use protocol::model::{
    AccountIdentity, BranchName, Branches, CacheTtl, ContextUsage, ConversationCache, GithubAppSaid,
    GithubRepositoryName, GithubUser, GithubUserId, HubUpdate, LimitWindow, LineCounts, Local, ModelName,
    ObservationKey, OtherBuild, OwnPullRequestDetail, OwnPullRequestSaid, OwnSummary, PromptCache, PullRequestDetail,
    PullRequestSaid, Reach, ReferenceSaid, ReleaseOffer, RepositoryUnseen, UsageLimit, Wait,
};

use super::kinds::{APP, chip, installation, models, s, v, w};
use crate::hub::tests::{changes, github_kept, hub_in_again, store_of};
use crate::hub::{self, Hub, Kind, SessionKey, WaitKey};
use crate::store::BEFORE_APPS_OBSERVED;
use crate::store::older::store_from_before;
use crate::store::tests::idle_session;

/// A hub with the workspaces [`v`] and `w`, whose session `s` has the open wait `a`, the app of your own [`APP`], whose
/// installation is the GitHub account [`installation`], and the agent's account `claude-ada`, each a record an
/// observation is of, made on a database from before the app's observations left it, as one from before can only be
/// given rows. Its cache of GitHub has the chip's issue.
pub(super) async fn hub_with_a_record_of_each(dir: &std::path::Path) -> Arc<Hub> {
    let rows = "INSERT INTO github_apps (id, slug, name, owner_id, owner, key_file, made_from)
                    VALUES (9, 'ada-roundhouse', 'Ada', 2, 'adalabs', 'a-file', 'a-code');
                INSERT INTO github_accounts (id, user_id, app_id, installation_id, installed_on_id, installed_on)
                    VALUES ('g', 41, 9, 70, 2, 'adalabs');
                INSERT INTO accounts (id) VALUES ('claude-ada');
                INSERT INTO workspaces (id, repo_id, name, branch, status, provider, instance_id, created_at_ms)
                    VALUES ('v', 'r', 'birch', 'birch', 'ready', 'local', 'v', 0);";
    let store = store_from_before(dir, BEFORE_APPS_OBSERVED, rows);
    store.insert_session(&idle_session(crate::store::now_ms()), None).unwrap();
    drop(store);
    let hub = hub_in_again(dir);
    github_kept(&hub, &chip(), protocol::fixtures::issue("example/project", 12, "Decide"));
    hub.start_wait(Wait { call: None, ..protocol::fixtures::pull_request_wait("a") }).await.unwrap();
    hub
}

/// Makes the observation `kind` say `round`, as whatever hears it in the hub would, where that tells clients of more,
/// and otherwise as the store's change of it alone.
#[expect(clippy::too_many_lines, reason = "a flat match over every kind, each arm a change of one observation")]
pub(super) async fn make(hub: &Hub, kind: &ObservationKey, round: u8) {
    use ObservationKey as K;
    let (said, at) = (round.to_string(), 1_000 * i64::from(round));
    let session = SessionKey { workspace_id: w(), session_id: s() };
    match kind {
        K::WorkspaceLines { .. } => {
            read::<hub::WorkspaceLines>(hub, v(), LineCounts { added: round.into(), removed: 0 }, at).await;
        }
        K::WorkspaceChanges { .. } => read::<hub::WorkspaceChanges>(hub, v(), changes(round.into(), 0), at).await,
        K::WorkspacePullRequest { .. } => {
            let pull_request = PullRequestSaid::of(&protocol::fixtures::pull_request(round.into()));
            let local = Local { uncommitted: 0, ahead: 0, behind: 0 };
            let said = OwnPullRequestSaid { pull_request: Some(OwnSummary { pull_request, local }) };
            worked_out::<hub::WorkspacePullRequest>(hub, v(), said, at).await;
        }
        K::WorkspacePullRequestDetail { .. } => {
            let detail = PullRequestDetail::of(&protocol::fixtures::pull_request(round.into()));
            let detail = OwnPullRequestDetail { pull_request: Some(detail) };
            worked_out::<hub::WorkspacePullRequestDetail>(hub, v(), detail, at).await;
        }
        K::WorkspaceRepositoryUnseen { .. } => {
            let unseen = RepositoryUnseen {
                repository: GithubRepositoryName::named("example/project"),
                username: said.into(),
                login: protocol::model::GithubLoginKind::Token,
                installation: None,
                said: "Not Found".into(),
                approve_at: None,
            };
            read::<hub::WorkspaceRepositoryUnseen>(hub, v(), Some(unseen), at).await;
        }
        K::Reference { .. } => {
            let issue = protocol::fixtures::issue("example/project", 12, &said);
            worked_out::<hub::Referenced>(hub, chip(), ReferenceSaid::of(Some(&issue)).unwrap(), at).await;
        }
        K::WorkspaceBranches { .. } => {
            let branches =
                Branches { head: protocol::model::Head::Branch { name: BranchName::from(said) }, had: Vec::new() };
            read::<hub::WorkspaceBranches>(hub, v(), branches, at).await;
        }
        K::WorkspaceWorkerBuild { .. } => {
            read::<hub::WorkspaceWorkerBuild>(hub, v(), OtherBuild { said: Some(said) }, at).await;
        }
        K::RepositoryReach { repo } => {
            let reach =
                Reach::Pushes { repository: GithubRepositoryName::named("example/project"), username: said.into() };
            read::<hub::RepositoryReach>(hub, repo.clone(), reach, at).await;
        }
        K::GithubUsername { .. } => read::<hub::GithubUsername>(hub, installation(), said.into(), at).await,
        K::GithubAppSaid { .. } => {
            let owner = GithubUser { id: GithubUserId::new(2), name: "adalabs".into() };
            let app = GithubAppSaid { slug: "ada-roundhouse".into(), name: said, owner };
            read::<hub::GithubAppSaid>(hub, APP, app, at).await;
        }
        K::GithubAppBot { .. } => {
            read::<hub::GithubAppBot>(hub, APP, GithubUser { id: GithubUserId::new(41), name: said.into() }, at).await;
        }
        K::InstalledOn { .. } => read::<hub::InstalledOn>(hub, installation(), said.into(), at).await,
        // As a lookup's answer, which changes the list's record too.
        K::ModelCatalog { .. } => {
            let catalog = ModelCatalog { offered: Vec::new(), default_model: Some(ModelName::try_from(said).unwrap()) };
            hub.model_list_answered(&models(), catalog).await;
        }
        K::AccountIdentity { account } => {
            let identity = AccountIdentity { plan: Some(said), ..AccountIdentity::default() };
            read::<hub::AccountIdentity>(hub, account.clone(), identity, at).await;
        }
        K::AccountLimits { account } => {
            let week = UsageLimit {
                window: LimitWindow::SevenDays,
                model: None,
                used_percent: round,
                resets_at_ms: None,
                heard_at_ms: None,
            };
            hub.hear_limits(account.clone(), vec![week], false, at).await;
        }
        K::SessionContext { .. } => {
            let context = ContextUsage {
                used_tokens: round.into(),
                max_tokens: 200_000,
                model_tokens: None,
                max_set_by: None,
                parts: Vec::new(),
            };
            read::<hub::SessionContext>(hub, session, context, at).await;
        }
        K::SessionTodos { .. } => {
            read::<hub::SessionTodos>(hub, session, TodoList { items: Vec::new(), note: Some(said) }, at).await;
        }
        K::SessionToolServers { .. } => {
            let state = ServerState::Connected;
            let server = ToolServerState { name: said.into(), hubs: None, state, error: None, tools: None };
            read::<hub::SessionToolServers>(hub, session, vec![server], at).await;
        }
        K::SessionCache { .. } => {
            let held = PromptCache { used_at_ms: at, ttl: CacheTtl::OneHour, model: None, tokens: round.into() };
            let cache = ConversationCache { held: Some(held), worth_telling: true };
            worked_out::<hub::SessionCache>(hub, session, cache, at).await;
        }
        K::WaitRead { wait, .. } => {
            let watching = Wait { call: None, ..protocol::fixtures::pull_request_wait("a") }.watch.unwrap();
            let read = protocol::model::WaitRead { read_error: Some(said), ..watching.read() };
            let of = WaitKey { workspace_id: w(), session_id: s(), wait: wait.clone() };
            worked_out::<hub::WaitRead>(hub, of, read, at).await;
        }
        // As the hub's read of the update feed, which tells of the record of how an update is going after it.
        K::ReleaseOffered => {
            read::<hub::ReleaseOffered>(hub, (), ReleaseOffer { version: Some(said) }, at).await;
            let rev = store_of(hub).next_rev().unwrap();
            hub.notify(HubNotice::HubUpdate(HubUpdate { looked_at_ms: at, problem: None, attempt: None, rev }));
        }
        K::Unknown => unreachable!("the hub makes no observation of a kind it doesn't know"),
    }
}

async fn read<K: Kind>(hub: &Hub, of: K::Of, value: K::Value, at: i64) {
    hub.observation_read::<K>(of, value, at).await.unwrap();
}

async fn worked_out<K: Kind>(hub: &Hub, of: K::Of, value: K::Value, at: i64) {
    hub.observation_worked_out::<K>(of, value, at).await.unwrap();
}
