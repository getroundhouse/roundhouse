//! Forgetting observations: every one of a workspace that's gone, whatever its kind, and those a caller picks out.

use std::sync::Arc;

use protocol::client::HubNotice;
use protocol::model::{
    GithubRepositoryName, IssueNumber, Level, ObservationKey, OwnPullRequestDetail, OwnPullRequestSaid, ReferenceAsk,
    ReferenceSaid, WorkspaceId,
};

use super::super::kinds::{
    Referenced, WorkspaceChanges, WorkspaceLines, WorkspacePullRequest, WorkspacePullRequestDetail,
    WorkspaceRepositoryUnseen,
};
use super::seen::{changes, lines, listed, told};
use crate::hub::Hub;
use crate::hub::tests::hub_in;

/// Keeps one observation of each kind of what's of a workspace, of `id`.
async fn one_of_each(hub: &Hub, id: &str) {
    let id = WorkspaceId::from(id);
    hub.observation_read::<WorkspaceLines>(id.clone(), lines(3, 1), 1_000).await.unwrap();
    hub.observation_read::<WorkspaceChanges>(id.clone(), changes(3, 1), 1_000).await.unwrap();
    let said = OwnPullRequestSaid { pull_request: None };
    hub.observation_worked_out::<WorkspacePullRequest>(id.clone(), said, 1_000).await.unwrap();
    let detail = OwnPullRequestDetail { pull_request: None };
    hub.observation_worked_out::<WorkspacePullRequestDetail>(id.clone(), detail, 1_000).await.unwrap();
    hub.observation_read::<WorkspaceRepositoryUnseen>(id, None, 1_000).await.unwrap();
}

/// How many kinds the store keeps an observation of `id` of.
fn kinds_kept(hub: &Hub, id: &str) -> usize {
    let (id, kept) = (WorkspaceId::from(id), &hub.observations);
    let kinds = [
        kept.keeps::<WorkspaceLines>(&id),
        kept.keeps::<WorkspaceChanges>(&id),
        kept.keeps::<WorkspacePullRequest>(&id),
        kept.keeps::<WorkspacePullRequestDetail>(&id),
        kept.keeps::<WorkspaceRepositoryUnseen>(&id),
    ];
    kinds.into_iter().filter(|kept| *kept).count()
}

/// A workspace that's gone takes every observation of it, of every kind keyed by a workspace, in one operation under
/// one revision, and leaves those of another workspace. Forgetting it again tells nothing.
#[tokio::test]
async fn a_workspace_that_s_gone_takes_every_kind_of_observation_of_it() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    one_of_each(&hub, "w").await;
    one_of_each(&hub, "v").await;
    let (_, list_rev) = listed(&hub).await;
    let mut feed = hub.subscribe_feed();

    hub.workspace_observations_gone(&"w".into()).await;
    hub.workspace_observations_gone(&"w".into()).await;

    let told = told(&mut feed);
    let removed: Vec<_> = told
        .iter()
        .map(|notice| {
            let HubNotice::ObservationRemoved { observation, rev } = notice else {
                panic!("not a removal: {notice:?}")
            };
            (observation.clone(), *rev)
        })
        .collect();
    let id = WorkspaceId::from("w");
    let keys: Vec<_> = removed.iter().map(|(key, _)| key.clone()).collect();
    assert_eq!(
        keys,
        [
            ObservationKey::WorkspaceLines { workspace_id: id.clone() },
            ObservationKey::WorkspaceChanges { workspace_id: id.clone() },
            ObservationKey::WorkspacePullRequest { workspace_id: id.clone() },
            ObservationKey::WorkspacePullRequestDetail { workspace_id: id.clone() },
            ObservationKey::WorkspaceRepositoryUnseen { workspace_id: id },
        ]
    );
    assert!(removed.iter().all(|(_, rev)| *rev == removed[0].1 && *rev > list_rev), "{removed:?}");
    assert_eq!(kinds_kept(&hub, "w"), 0);
    assert_eq!(kinds_kept(&hub, "v"), 5, "another workspace's");
    let (at_its_level, _) = hub.observations_at(&Level::Workspace { id: "w".into() }).await.unwrap();
    assert_eq!(at_its_level, []);
}

/// Those of a workspace the hub doesn't have go, as a hub that stopped before forgetting them leaves them, and those of
/// a workspace it has, and of a chip, which is of none, stay.
#[tokio::test]
async fn the_observations_of_a_workspace_the_hub_doesn_t_have_are_swept() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    one_of_each(&hub, "w").await;
    one_of_each(&hub, "gone").await;
    let chip = ReferenceAsk {
        repository: GithubRepositoryName::named("example/project"),
        number: IssueNumber::try_from(7).unwrap(),
    };
    hub.observation_worked_out::<Referenced>(chip.clone(), ReferenceSaid::Missing, 1_000).await.unwrap();

    hub.observations_of_no_workspace_gone().await;

    assert_eq!(kinds_kept(&hub, "gone"), 0);
    assert_eq!(kinds_kept(&hub, "w"), 5, "of a workspace it has");
    assert!(hub.observations.keeps::<Referenced>(&chip), "a chip's");
}

/// A read that found itself still wanted, and is being kept as the caller stops wanting it and forgets what's kept, is
/// forgotten with the rest, since what's forgotten is picked out under the lock the read holds, as when the hub lets go
/// of a list of models while its agent's answer lands.
#[tokio::test]
async fn a_read_still_landing_as_what_it_s_of_is_forgotten_goes_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let (wanted, asked) = tokio::sync::oneshot::channel();
    let reading = tokio::spawn({
        let hub = Arc::clone(&hub);
        async move {
            let still = || wanted.send(()).is_ok();
            hub.observation_read_while::<WorkspaceLines>("w".into(), lines(3, 1), 1_000, still).await.unwrap();
        }
    });
    asked.await.unwrap();

    hub.observations_gone_where::<WorkspaceLines>(|_| true).await.unwrap();
    reading.await.unwrap();

    assert!(!hub.observations.keeps::<WorkspaceLines>(&"w".into()), "forgotten");
}
