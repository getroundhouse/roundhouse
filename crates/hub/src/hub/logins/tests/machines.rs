//! A chat whose first launch, while its kind had no default, ran under its machine's own login, or found nobody logged
//! in there, and what its launches are handed once the hub holds a login.

use std::time::Duration;

use protocol::model::AccountSource;
use protocol::worker::WorkerRequest;

use super::two_accounts::{Chats, cold};
use crate::hub::stand_in_workers::{a_chat, logged_in, logged_in_as};

/// How long the hub has to learn whose a launch's login is, which it does on a task of its own as the agent starts.
const TIMEOUT: Duration = Duration::from_secs(10);

/// On a machine with an ambient login, a chat's first launch while its kind has no default puts it on that login, and
/// it stays there once the hub holds a login, which a new chat is put on instead. Moving it puts it on the hub's, and
/// that launch's first request isn't taken for one that found the cache expired, since the ambient account's cache is
/// another.
#[tokio::test]
async fn a_chat_on_the_ambient_login_stays_on_it_once_a_login_is_added_until_its_moved() {
    let dir = tempfile::tempdir().unwrap();
    let chats = Chats::new(dir.path());
    let (workspace, chat) = a_chat(&chats.hub).await;
    logged_in(&chats.hub, &workspace.id, &chats.asked, |_| Ok(logged_in_as("ada@example.com", "Max")));
    let first = chats.token_in_turn(&chat.id, Some(cold())).await;
    // Only the worker says whose an ambient login is, so the chat names it a moment after its agent launches.
    let named = || chats.record(&chat.id).account.is_some();
    waiting::task::until(TIMEOUT, "the chat to name the ambient login's account", named).await;
    let ambient = chats.record(&chat.id);

    let work = chats.connect("Work").await;
    chats.crash(&chat.id).await;
    let still_ambient = chats.token_in_turn(&chat.id, None).await;
    let (_, new_chat) = a_chat(&chats.hub).await;
    let under_the_default = chats.token_in_turn(&new_chat.id, None).await;
    chats.assign(&chat.id, &work.id, false).await.unwrap();
    let under_work = chats.token_in_turn(&chat.id, Some(cold())).await;

    assert_eq!(first, None, "it was handed no login");
    assert_eq!((ambient.account_source, ambient.hub_account()), (Some(AccountSource::Ambient), None));
    assert_eq!(still_ambient, None, "it was handed no login");
    assert_eq!(chats.launches(&chat.id), [false, true, true], "launched again, and again once it was moved");
    assert_eq!(under_the_default.as_deref(), Some("work-token"));
    assert_eq!(under_work.as_deref(), Some("work-token"));
    assert_eq!(chats.on(&chat.id), Some(work.id));
    assert_eq!(chats.found_expired(&chat.id), []);
}

/// A chat whose first launch, while its kind had no default, found nobody logged in on its machine isn't put on the
/// ambient login, since there's no account to show it under, and so nothing to move it off from. So once a login is
/// added, its next launch puts it on that one. That launch's first request isn't taken for one that found the cache
/// expired, since the record named no account, so it can't say whose the cache was.
#[tokio::test]
async fn a_chat_whose_first_launch_found_nobody_logged_in_is_put_on_the_default_once_one_is_added() {
    let dir = tempfile::tempdir().unwrap();
    let chats = Chats::new(dir.path());
    let (_, chat) = a_chat(&chats.hub).await;
    let first = chats.token_in_turn(&chat.id, Some(cold())).await;
    // The worker is asked whose the machine's login is as the agent starts. Its answer changes nothing on the chat's
    // record, so there's nothing later to wait for: the hub files it as soon as it comes, long before the launch below.
    let whose = |request: &WorkerRequest| matches!(request, WorkerRequest::AccountFor(_)).then_some(());
    let asked_whose = || !chats.asked.requests(whose).is_empty();
    waiting::task::until(TIMEOUT, "the worker to be asked whose the machine's login is", asked_whose).await;
    let nobody = chats.record(&chat.id);

    let work = chats.connect("Work").await;
    chats.crash(&chat.id).await;
    let under_work = chats.token_in_turn(&chat.id, Some(cold())).await;

    assert_eq!(first, None, "it was handed no login");
    assert_eq!((nobody.account_source, nobody.account), (None, None));
    assert!(nobody.prompt_cache.is_some(), "its first turn left something cached");
    assert_eq!(under_work.as_deref(), Some("work-token"));
    assert_eq!(chats.on(&chat.id), Some(work.id));
    assert_eq!(chats.found_expired(&chat.id), []);
}
