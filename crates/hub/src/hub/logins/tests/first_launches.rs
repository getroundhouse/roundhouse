//! Which account a chat's first launch puts it on, when every client hears it was used, and a chat whose account, or
//! whose kind's default, can't be launched under.

use std::sync::Arc;

use parking_lot::Mutex;
use protocol::client::HubNotice;
use protocol::client::accounts::AccountSetDefault;
use protocol::model::{AccountId, AgentKind, TitleSource};
use tokio::sync::broadcast;

use super::two_accounts::{Chats, TwoAccounts, announced_on};
use crate::hub::stand_in_workers::{TIMEOUT, a_chat, titling};

/// When the feed next announces the account `id` as used after `after_ms`.
async fn used_after(feed: &mut broadcast::Receiver<HubNotice>, id: &AccountId, after_ms: i64) -> i64 {
    loop {
        let told = tokio::time::timeout(TIMEOUT, feed.recv()).await.expect("the account to be announced as used");
        if let HubNotice::Account(told) = told.expect("the feed goes on")
            && told.id == *id
            && let Some(used) = told.used_at_ms.filter(|used| *used > after_ms)
        {
            return used;
        }
    }
}

/// A chat's first launch under a held login puts it on that login's account, its kind's default, and every client
/// hears so as it launches, before its worker has answered the launch, and so before its turn ends. Choosing another
/// default then changes only which account new chats are put on.
#[tokio::test(flavor = "multi_thread")]
async fn a_chat_is_put_on_the_default_as_it_first_launches_and_stays_on_it_when_another_is_chosen() {
    let dir = tempfile::tempdir().unwrap();
    let accounts = TwoAccounts::start(dir.path()).await;
    let chats = &accounts.chats;
    let fresh = accounts.another_chat().await;
    let before = chats.on(&fresh);
    let mut feed = chats.hub.subscribe_feed();
    let (answer_the_launch, held) = std::sync::mpsc::channel::<()>();
    let held_for = fresh.clone();
    // Its worker holds its launch until the test lets it go, or for twice the test's limit, and then answers it.
    crate::hub::stand_in_workers::answering_first(&chats.hub, &accounts.workspace.id, &chats.asked, move |request| {
        let protocol::worker::WorkerRequest::AgentStart(spec) = request else { return None };
        if spec.session_id == held_for {
            let _ = tokio::task::block_in_place(|| held.recv_timeout(TIMEOUT * 2));
        }
        None
    });

    // Its agent takes the prompt and is still at work on it.
    chats.hub.session_prompt(fresh.clone(), None, protocol::prompt::Prompt::new("work")).await.unwrap();
    let announced = announced_on(&mut feed, &fresh).await;
    answer_the_launch.send(()).unwrap();
    let given = || !chats.asked.prompts_given(&fresh).is_empty();
    waiting::task::until(TIMEOUT, "the prompt to be given once the launch was answered", given).await;
    let personal = AccountSetDefault { agent: AgentKind::Claude, account: accounts.personal.id.clone() };
    chats.hub.account_set_default(personal).await.unwrap();
    chats.crash(&accounts.chat).await;
    let relaunched = chats.token_in_turn(&accounts.chat, None).await;
    let newer = accounts.another_chat().await;
    let under_new_default = chats.token_in_turn(&newer, None).await;

    assert_eq!(chats.on(&accounts.chat), Some(accounts.work.id.clone()), "its first launch put it on Work");
    assert_eq!((before, announced), (None, accounts.work.id.clone()));
    assert!(chats.record(&fresh).status.is_working(), "it was announced before its turn ended");
    assert_eq!(relaunched.as_deref(), Some("work-token"), "it stayed on Work");
    assert_eq!(chats.launches(&accounts.chat), [false, true], "it was launched again");
    assert_eq!(under_new_default.as_deref(), Some("personal-token"));
    assert_eq!(chats.on(&newer), Some(accounts.personal.id.clone()));
}

#[tokio::test]
async fn a_chat_whose_account_was_disconnected_is_blocked_rather_than_left_to_the_default() {
    let dir = tempfile::tempdir().unwrap();
    let accounts = TwoAccounts::start(dir.path()).await;
    let (chats, chat) = (&accounts.chats, &accounts.chat);
    chats.assign(chat, &accounts.personal.id, false).await.unwrap();
    chats.hub.account_disconnect(accounts.personal.id.clone()).await.unwrap();

    let failed = chats.refused_launch(chat).await;
    let launched_while_blocked = chats.launches(chat);
    let asked_while_blocked = chats.asked.launches(chat).len();
    let moved_back = chats.assign(chat, &accounts.work.id, false).await.unwrap();
    let under_work = chats.token_in_turn(chat, None).await;

    assert!(failed.contains("this chat's account Personal needs a login"), "{failed}");
    assert_eq!(launched_while_blocked, [false], "nothing launched under the default meanwhile");
    assert_eq!(asked_while_blocked, 1, "no worker was asked to launch one");
    assert_eq!(moved_back.hub_account(), Some(&accounts.work.id));
    assert_eq!(under_work.as_deref(), Some("work-token"));
}

/// A new chat whose default can't be used isn't put on it, since its launch fails before it's given a login, so it
/// takes another default once one is chosen. A chat already on that account stays blocked until it's moved.
#[tokio::test]
async fn a_new_chat_isnt_put_on_a_default_it_cant_launch_under_and_choosing_another_unblocks_only_new_chats() {
    let dir = tempfile::tempdir().unwrap();
    let accounts = TwoAccounts::start(dir.path()).await;
    let (chats, chat) = (&accounts.chats, &accounts.chat);
    let fresh = accounts.another_chat().await;
    chats.hub.account_disconnect(accounts.work.id.clone()).await.unwrap();

    let fresh_failed = chats.refused_launch(&fresh).await;
    let fresh_on = chats.on(&fresh);
    let personal = AccountSetDefault { agent: AgentKind::Claude, account: accounts.personal.id.clone() };
    chats.hub.account_set_default(personal).await.unwrap();
    let fresh_under = chats.token_in_turn(&fresh, None).await;
    // Its agent has gone on under the login it was given, as an agent does once its login is disconnected.
    chats.crash(chat).await;
    let chat_failed = chats.refused_launch(chat).await;

    assert!(fresh_failed.contains("the default account Work needs a login"), "{fresh_failed}");
    assert_eq!(fresh_on, None);
    assert_eq!(fresh_under.as_deref(), Some("personal-token"));
    assert_eq!(chats.on(&fresh), Some(accounts.personal.id.clone()));
    assert!(chat_failed.contains("this chat's account Work needs a login"), "{chat_failed}");
    assert_eq!(chats.on(chat), Some(accounts.work.id.clone()));
    assert_eq!(chats.asked.launches(chat).len(), 1, "it wasn't launched under the new default");
}

/// A chat's title is asked for under the login its launches take, so a chat moved to an account before its first
/// prompt is titled under that account's, and its first launch leaves it there.
#[tokio::test]
async fn a_chat_moved_before_its_first_prompt_is_titled_under_its_accounts_login_and_stays_on_it() {
    let dir = tempfile::tempdir().unwrap();
    let accounts = TwoAccounts::start(dir.path()).await;
    let chats = &accounts.chats;
    let fresh = accounts.another_chat().await;
    let titled_under = Arc::new(Mutex::new(Vec::new()));
    let kept = titled_under.clone();
    titling(&chats.hub, &accounts.workspace.id, &chats.asked, move |call| {
        kept.lock().push(call.login.as_ref().map(|login| login.token.as_str().to_string()));
        Ok("Under Personal".to_string())
    });

    chats.assign(&fresh, &accounts.personal.id, false).await.unwrap();
    let launched_under = chats.token_in_turn(&fresh, None).await;
    let titled = || chats.record(&fresh).title_source == TitleSource::Generated;
    waiting::task::until(TIMEOUT, "the chat's generated title", titled).await;

    assert_eq!(chats.record(&fresh).title.as_deref(), Some("Under Personal"));
    assert_eq!(*titled_under.lock(), [Some("personal-token".to_string())]);
    assert_eq!(launched_under.as_deref(), Some("personal-token"));
    assert_eq!(chats.on(&fresh), Some(accounts.personal.id.clone()));
}

/// Every client hears when a chat's account was last used: by its first prompt, once the launch that prompt brought
/// puts the chat on the account, and then by each prompt sent. Nobody is logged in on the machine, so no account of its
/// own is filed beside the login's.
#[tokio::test]
async fn a_chats_account_is_announced_as_used_by_its_first_prompt_once_its_launch_names_it_and_by_each_after() {
    let dir = tempfile::tempdir().unwrap();
    let chats = Chats::new(dir.path());
    let work = chats.connect("Work").await;
    let (_, chat) = a_chat(&chats.hub).await;
    let mut feed = chats.hub.subscribe_feed();

    let first_under = chats.token_in_turn(&chat.id, None).await;
    let first = used_after(&mut feed, &work.id, 0).await;
    // A use is when the prompt was recorded, to the millisecond, and only a later one is news, so the next prompt is
    // sent once the clock has moved past the first.
    let later = || crate::store::now_ms() > first;
    waiting::task::until(TIMEOUT, "the clock to move past the first prompt", later).await;
    chats.token_in_turn(&chat.id, None).await;
    let second = used_after(&mut feed, &work.id, first).await;

    assert_eq!(first_under.as_deref(), Some("work-token"));
    assert!(second > first, "the next prompt was announced as a use: {first} then {second}");
    assert_eq!(chats.on(&chat.id), Some(work.id));
    assert_eq!(chats.hub.store.list_records().unwrap().accounts.len(), 1, "no account but the login's");
}
