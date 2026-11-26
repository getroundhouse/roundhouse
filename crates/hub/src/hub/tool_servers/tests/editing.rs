//! Editing a server by signing in to it, from its start to what its approval comes to. What's kept, in what order, and
//! what's refused is tested in `crate::tool_servers`.

use std::path::Path;

use protocol::client::ErrorCode;
use protocol::client::tool_servers::{ToolServerAdd, ToolServerEdit};
use protocol::client::tool_sign_ins::{ToolSignInBegun, ToolSignInStart, ToolSignInTo};
use protocol::model::{ToolServer, ToolServerAuthGiven, ToolServerKey};
use tool_sign_in::{Asks, Person, Script, ServerMetadataAt, StandIn};

use super::support::{WORKS, access, finished, kept, page, servers, signed_in, start, told, visited};
use crate::hub::Hub;
use crate::hub::tests::{hub_in, hub_in_again};

/// Signing in, as `id`, to have the server `t` be at `stand_in`'s address, called `Linear (EU)`.
fn edit(id: &str, stand_in: &StandIn) -> ToolSignInStart {
    let url = Some(stand_in.server().as_str().to_string());
    start(id, ToolSignInTo::Edit { id: "t".into(), name: Some("Linear (EU)".into()), url })
}

/// A hub whose server `t` is signed in to at the first of two stand-ins.
async fn at_the_first(dir: &Path) -> (std::sync::Arc<Hub>, StandIn, StandIn) {
    let (hub, first, second) =
        (hub_in(dir), StandIn::with(Script::default()).await, StandIn::with(Script::default()).await);
    signed_in(&hub, &first).await;
    (hub, first, second)
}

fn is_the_second(server: &ToolServer, second: &StandIn) {
    assert_eq!((server.id.as_str(), server.name.as_str(), &server.auth), ("t", "Linear (EU)", &WORKS));
    assert_eq!(server.url.as_str(), second.server().as_str());
}

#[tokio::test]
async fn a_server_moves_once_its_approved_and_every_client_is_told_and_finishing_it_again_is_the_same_after_a_restart()
{
    let dir = tempfile::tempdir().unwrap();
    let (hub, first, second) = at_the_first(dir.path()).await;
    let (before, held) = (servers(&hub), access(dir.path()));
    let mut feed = hub.subscribe_feed();

    let page = page(&hub, edit("edit", &second)).await;
    let while_it_waits = (servers(&hub), access(dir.path()), told(&mut feed));
    let finishing = visited("edit", &second, &page);
    let server = hub.tool_sign_in_finish(finishing.clone()).await.unwrap();
    let told = told(&mut feed);
    let again = hub.tool_sign_in_finish(finishing.clone()).await.unwrap();
    drop(hub);
    let after_a_restart = hub_in_again(dir.path()).tool_sign_in_finish(finishing).await.unwrap();

    assert_eq!(while_it_waits, (before, held, vec![]), "nothing changes until it's approved");
    is_the_second(&server, &second);
    assert_eq!(told, std::slice::from_ref(&server));
    assert_eq!((&again, &after_a_restart), (&server, &server));
    assert_eq!(kept(dir.path())["made_by"], "edit", "in place of the one before");
    assert_eq!(first.asked_for("POST /token").len(), 1, "the one it was is asked nothing more");
    assert_eq!(second.asked_for("POST /token").len(), 1);
}

#[tokio::test]
async fn one_refused_cancelled_or_with_no_way_to_sign_in_leaves_the_server_and_its_sign_in_as_they_were() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _first, second) = at_the_first(dir.path()).await;
    let before = (servers(&hub), access(dir.path()));
    let as_it_was = |hub: &Hub| assert_eq!((servers(hub), access(dir.path())), before);

    second.change(|script| script.person = Person::Refuses);
    let refused = finished(&hub, &second, edit("refused", &second)).await.unwrap_err();
    assert_eq!(refused.code, ErrorCode::InvalidRequest);
    as_it_was(&hub);

    second.change(|script| script.person = Person::Approves);
    let waiting = page(&hub, edit("cancelled", &second)).await;
    hub.tool_sign_in_cancel(&"cancelled".into());
    let cancelled = hub.tool_sign_in_finish(visited("cancelled", &second, &waiting)).await.unwrap_err();
    assert_eq!(cancelled.code, ErrorCode::NotFound);
    as_it_was(&hub);

    second.change(|script| (script.asks, script.server_metadata_at) = (Asks::Nothing, ServerMetadataAt::Nowhere));
    let begun = hub.tool_sign_in_start(edit("open", &second)).await.unwrap();
    assert_eq!(begun, ToolSignInBegun::NoSignIn);
    as_it_was(&hub);
}

#[tokio::test]
async fn one_whose_server_is_removed_or_signed_in_to_meanwhile_is_refused_as_its_finished() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, first, second) = at_the_first(dir.path()).await;
    let across_another = page(&hub, edit("late", &second)).await;
    let sooner = finished(&hub, &first, {
        let url = Some(format!("{}?tools=some", first.server().as_str()));
        start("sooner", ToolSignInTo::Edit { id: "t".into(), name: Some("Sentry".into()), url })
    });
    let edited = sooner.await.unwrap();

    let late = hub.tool_sign_in_finish(visited("late", &second, &across_another)).await.unwrap_err();
    assert_eq!(late.code, ErrorCode::Conflict);
    assert_eq!(servers(&hub), [edited]);

    let across_its_removal = page(&hub, edit("removed", &second)).await;
    hub.tool_server_remove("t".into()).await.unwrap();
    let removed = hub.tool_sign_in_finish(visited("removed", &second, &across_its_removal)).await.unwrap_err();
    assert_eq!(removed.code, ErrorCode::Conflict);
    assert_eq!(servers(&hub), []);
    assert_eq!(std::fs::read_dir(dir.path().join("tool-servers")).unwrap().count(), 0, "nothing of it is kept");
}

/// And edited without signing in, told once, and only where that changed it.
#[tokio::test]
async fn a_server_with_a_key_comes_to_be_signed_in_to_and_every_client_is_told_of_each_edit_that_changed_it() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, stand_in) = (hub_in(dir.path()), StandIn::with(Script::default()).await);
    let auth = ToolServerAuthGiven::Key { header: "X-Key".into(), key: ToolServerKey::new("made-up-key") };
    let url = stand_in.server().as_str().to_string();
    let add = ToolServerAdd { id: "t".into(), name: "Linear".into(), url, auth };
    hub.tool_server_add(add).await.unwrap();
    let mut feed = hub.subscribe_feed();
    let rename = || ToolServerEdit { id: "t".into(), name: Some("Linear (EU)".into()), url: None, auth: None };

    let renamed = hub.tool_server_edit(rename()).await.unwrap();
    hub.tool_server_edit(rename()).await.unwrap();
    let to = ToolSignInTo::Edit { id: "t".into(), name: None, url: None };
    let signed_in = finished(&hub, &stand_in, start("i", to)).await.unwrap();

    is_the_second(&signed_in, &stand_in);
    assert_eq!(told(&mut feed), [renamed, signed_in]);
    let held = std::fs::read_dir(dir.path().join("tool-servers")).unwrap().count();
    assert_eq!(held, 1, "its key is forgotten");
    assert!(access(dir.path()).is_some());
}
