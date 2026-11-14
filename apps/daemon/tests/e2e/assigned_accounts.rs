//! The account a chat is on, and moving it to another the hub holds a login for, through a real hub, mostly on a
//! machine with no login of its own, so every launch runs under a login the hub hands out.

#![cfg(test)]

use client_core::{Client, ClientError, Subscription};
use protocol::client::{ErrorCode, HubNotice};
use protocol::event::{EventBody, ExitReason, SessionEvent};
use protocol::model::{Account, AccountId, AgentKind, LoginToken, SessionId, SessionSettingsChange, SessionStatus};

use crate::accounts::log_out;
use crate::support::gh::{MERGED, OPEN, StandInGh};
use crate::support::{Stack, assistant_text, next_notice, read_all, wait_for_status, wait_for_turn_end, wait_until};

const TOKEN: &str = "env:CLAUDE_CODE_OAUTH_TOKEN";
const WAIT: &str = r#"mcp:wait_for_pull_requests {"pull_requests":[7],"until":"merged"}"#;
const READ: &str = r#"mcp:get_pull_requests {"pull_requests":[7]}"#;

/// A stack whose machine has no login, with two accounts the hub holds, Work, which is the default as the first added,
/// and Personal, and a chat that has had a turn under Work, which put it on Work.
struct TwoAccounts {
    stack: Stack,
    work: Account,
    personal: Account,
    session: SessionId,
}

impl TwoAccounts {
    fn start() -> TwoAccounts {
        TwoAccounts::on(Stack::start())
    }

    fn on(stack: Stack) -> TwoAccounts {
        log_out(&stack);
        let client = stack.client();
        let (work, personal) = (connect(&client, "Work"), connect(&client, "Personal"));
        let (_, session) = stack.workspace(&client);
        assert_eq!(token_in_turn(&client, &session.id, 1, "say:hello"), "work-token");
        TwoAccounts { stack, work, personal, session: session.id }
    }

    fn client(&self) -> Client {
        self.stack.client()
    }

    /// Another chat in the same workspace, on no account until its agent first starts.
    fn another_chat(&self, client: &Client) -> SessionId {
        let workspace = client.session_get(&self.session).unwrap().workspace_id;
        client.session_create(&workspace, AgentKind::Claude, SessionSettingsChange::default(), None).unwrap().id
    }
}

/// Has the hub hold a login for Claude called `name`, whose token is its name in lower case, then `-token`.
fn connect(client: &Client, name: &str) -> Account {
    let token = LoginToken::new(format!("{}-token", name.to_lowercase()));
    client.account_connect(AgentKind::Claude, name, token, None).unwrap()
}

/// The account the chat is on, as the hub records it.
fn on(client: &Client, session: &SessionId) -> Option<AccountId> {
    client.session_get(session).unwrap().assigned_account
}

/// The token the agent runs under, as it says once it has done `then` in the `turn`th turn.
fn token_in_turn(client: &Client, session: &SessionId, turn: usize, then: &str) -> String {
    client.session_prompt(session, &format!("{then};{TOKEN}"), None).unwrap();
    let text = assistant_text(&wait_for_turn_end(client, session, turn));
    text.rsplit("CLAUDE_CODE_OAUTH_TOKEN=").next().unwrap().to_string()
}

/// Has the agent exit, so the next prompt launches another under whatever account the chat runs under by then.
fn crash(client: &Client, session: &SessionId) {
    client.session_prompt(session, "crash:3", None).unwrap();
    wait_for_status(client, session, SessionStatus::Interrupted);
}

/// Prompts a chat whose launch is refused, and says why, as the launch's failure does.
fn refused_launch(client: &Client, session: &SessionId) -> String {
    client.session_prompt(session, TOKEN, None).unwrap();
    let mut failed = None;
    wait_until("the launch to fail", || {
        failed = read_all(client, session).into_iter().find_map(|e| match e.body {
            EventBody::AgentExited { reason: ExitReason::StartFailed, stderr_tail, .. } => Some(stderr_tail),
            _ => None,
        });
        failed.is_some()
    });
    failed.unwrap().join("\n")
}

/// Whether each launch resumed the conversation, in order.
fn launches(events: &[SessionEvent]) -> Vec<bool> {
    let resumed = |e: &SessionEvent| match e.body {
        EventBody::AgentStarted { resumed, .. } => Some(resumed),
        _ => None,
    };
    events.iter().filter_map(resumed).collect()
}

/// The account the feed first announces `session` as on.
fn announced_on(feed: &Subscription, session: &SessionId) -> AccountId {
    loop {
        match next_notice(feed, "the chat to be announced as on an account") {
            HubNotice::Session(told) if told.id == *session && told.assigned_account.is_some() => {
                return told.assigned_account.unwrap();
            }
            _ => {}
        }
    }
}

/// A chat's first launch under a held login puts it on that login's account, its kind's default, and every client
/// hears so as it launches, before its turn ends. Choosing another default then changes only which account new chats
/// are put on.
#[test]
fn a_chat_is_put_on_the_default_as_it_first_launches_and_stays_on_it_when_another_is_chosen() {
    let chat = TwoAccounts::start();
    let client = chat.client();
    let fresh = chat.another_chat(&client);
    let before = on(&client, &fresh);
    let feed = client.feed_subscribe().unwrap();

    client.session_prompt(&fresh, "hang:60000", None).unwrap();
    let announced = announced_on(&feed, &fresh);
    client.session_cancel(&fresh).unwrap();
    client.account_set_default(AgentKind::Claude, &chat.personal.id).unwrap();
    crash(&client, &chat.session);
    let relaunched = token_in_turn(&client, &chat.session, 2, "say:again");
    let newer = chat.another_chat(&client);
    let under_new_default = token_in_turn(&client, &newer, 1, "say:hello");

    assert_eq!(on(&client, &chat.session), Some(chat.work.id.clone()), "its first launch put it on Work");
    assert_eq!((before, announced), (None, chat.work.id.clone()));
    assert_eq!(relaunched, "work-token", "it stayed on Work");
    assert_eq!(under_new_default, "personal-token");
    assert_eq!(on(&client, &newer), Some(chat.personal.id.clone()));
}

#[test]
fn a_moved_chat_resumes_under_its_account_and_goes_back_once_moved_back() {
    let chat = TwoAccounts::start();
    let client = chat.client();

    let moved = client.session_assign_account(&chat.session, &chat.personal.id, false).unwrap();
    let under_personal = token_in_turn(&client, &chat.session, 2, "recall");
    let recalled = assistant_text(&read_all(&client, &chat.session));
    let back = client.session_assign_account(&chat.session, &chat.work.id, false).unwrap();
    let under_work = token_in_turn(&client, &chat.session, 3, "say:back");

    assert_eq!(moved.assigned_account, Some(chat.personal.id.clone()));
    assert_eq!(moved.account, Some(chat.personal.id.clone()), "the record names the account it goes on under");
    assert_eq!(under_personal, "personal-token");
    assert!(recalled.contains(&format!("I have seen: say:hello;{TOKEN}")), "it carried on: {recalled}");
    assert_eq!((back.assigned_account, back.account), (Some(chat.work.id.clone()), Some(chat.work.id.clone())));
    assert_eq!(under_work, "work-token");
    assert_eq!(launches(&read_all(&client, &chat.session)), [false, true, true], "each move resumed it");
}

/// Which account a chat is on is the hub's record, so the next hub launches it under the same one.
#[test]
fn a_chat_stays_on_the_account_it_was_moved_to_once_the_hub_restarts() {
    let mut chat = TwoAccounts::start();
    chat.client().session_assign_account(&chat.session, &chat.personal.id, false).unwrap();

    chat.stack.stop_hub();
    chat.stack.start_hub();
    let under = token_in_turn(&chat.client(), &chat.session, 2, "say:after the restart");

    assert_eq!(under, "personal-token");
}

#[test]
fn a_chat_moved_to_the_account_its_on_keeps_its_agent() {
    let chat = TwoAccounts::start();
    let client = chat.client();

    let moved = client.session_assign_account(&chat.session, &chat.work.id, false).unwrap();
    let kept = token_in_turn(&client, &chat.session, 2, "say:same agent");

    assert_eq!((moved.assigned_account, moved.account), (Some(chat.work.id.clone()), Some(chat.work.id)));
    assert_eq!(kept, "work-token");
    assert_eq!(launches(&read_all(&client, &chat.session)), [false], "nothing relaunched it");
}

#[test]
fn moving_a_chat_whose_agent_is_at_work_needs_force_and_leaves_it_stopped_short() {
    let chat = TwoAccounts::start();
    let client = chat.client();
    client.session_prompt(&chat.session, "hang:60000", None).unwrap();
    wait_until("the long tool to start", || {
        read_all(&client, &chat.session).iter().filter(|e| matches!(e.body, EventBody::Assistant { .. })).count() > 1
    });

    let refused = client.session_assign_account(&chat.session, &chat.personal.id, false).unwrap_err();
    let unmoved = client.session_get(&chat.session).unwrap();
    let moved = client.session_assign_account(&chat.session, &chat.personal.id, true).unwrap();
    wait_until("the chat to stop short", || client.session_get(&chat.session).unwrap().stopped_short());
    client.session_prompt(&chat.session, TOKEN, None).unwrap();
    wait_until("a turn under the account it was moved to", || {
        assistant_text(&read_all(&client, &chat.session)).ends_with("CLAUDE_CODE_OAUTH_TOKEN=personal-token")
    });

    assert!(matches!(&refused, ClientError::Api(e) if e.code == ErrorCode::NeedsForce), "{refused}");
    let work = Some(chat.work.id.clone());
    assert_eq!((unmoved.assigned_account, unmoved.account), (work.clone(), work), "nothing changed");
    assert_eq!((moved.assigned_account, moved.account), (Some(chat.personal.id.clone()), Some(chat.personal.id)));
    assert_eq!(launches(&read_all(&client, &chat.session)), [false, true]);
}

#[test]
fn a_chat_whose_account_was_disconnected_is_blocked_rather_than_left_to_the_default() {
    let chat = TwoAccounts::start();
    let client = chat.client();
    client.session_assign_account(&chat.session, &chat.personal.id, false).unwrap();
    client.account_disconnect(&chat.personal.id).unwrap();

    let failed = refused_launch(&client, &chat.session);
    let launched_while_blocked = launches(&read_all(&client, &chat.session));
    let moved_back = client.session_assign_account(&chat.session, &chat.work.id, false).unwrap();
    let under_work = token_in_turn(&client, &chat.session, 2, "say:on Work again");

    assert!(failed.contains("this chat's account Personal needs a login"), "{failed}");
    assert_eq!(launched_while_blocked, [false], "nothing launched under the default meanwhile");
    assert_eq!(moved_back.assigned_account, Some(chat.work.id));
    assert_eq!(under_work, "work-token");
}

/// A new chat whose default can't be used isn't put on it, since its launch fails before it's given a login, so it
/// takes another default once one is chosen. A chat already on that account stays blocked until it's moved.
#[test]
fn a_new_chat_isnt_put_on_a_default_it_cant_launch_under_and_choosing_another_unblocks_only_new_chats() {
    let chat = TwoAccounts::start();
    let client = chat.client();
    let fresh = chat.another_chat(&client);
    client.account_disconnect(&chat.work.id).unwrap();

    let fresh_failed = refused_launch(&client, &fresh);
    let fresh_on = on(&client, &fresh);
    client.account_set_default(AgentKind::Claude, &chat.personal.id).unwrap();
    let fresh_under = token_in_turn(&client, &fresh, 1, "say:on the new default");
    // Its agent has gone on under the login it was given, as an agent does once its login is disconnected.
    crash(&client, &chat.session);
    let chat_failed = refused_launch(&client, &chat.session);

    assert!(fresh_failed.contains("the default account Work needs a login"), "{fresh_failed}");
    assert_eq!(fresh_on, None);
    assert_eq!(fresh_under, "personal-token");
    assert_eq!(on(&client, &fresh), Some(chat.personal.id));
    assert!(chat_failed.contains("this chat's account Work needs a login"), "{chat_failed}");
    assert_eq!(on(&client, &chat.session), Some(chat.work.id));
}

/// Another account's cache holds none of the conversation, so the first request under it writes the conversation
/// again, as one does once the cache has expired. A move isn't taken for that.
#[test]
fn the_first_request_after_a_move_isnt_taken_for_one_that_found_the_cache_expired() {
    let chat = TwoAccounts::start();
    let client = chat.client();

    let moved = client.session_assign_account(&chat.session, &chat.personal.id, false).unwrap();
    let after_the_move = token_in_turn(&client, &chat.session, 2, "cold");

    assert_eq!(moved.prompt_cache, None, "the move forgot the cache");
    assert_eq!(after_the_move, "personal-token");
    let events = read_all(&client, &chat.session);
    let expired: Vec<_> = events.iter().filter(|e| matches!(e.body, EventBody::CacheExpired { .. })).collect();
    assert!(expired.is_empty(), "{expired:?}");
}

/// On a machine with its own login, a chat whose agent ran under it is on no account, and once the hub holds a login,
/// its next launch puts it on that one. That launch's first request isn't taken for one that found the cache expired
/// either, since the machine's account's cache is another.
#[test]
fn a_chat_that_ran_under_the_machines_login_is_put_on_a_held_one_without_its_cache_taken_for_expired() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    // Only the worker says whose a login on its machine is, so the chat names it a moment after its agent launches.
    wait_until("the chat to name the machine's account", || client.session_get(&session.id).unwrap().account.is_some());
    let under_the_machine = client.session_get(&session.id).unwrap();

    let work = connect(&client, "Work");
    crash(&client, &session.id);
    let under_work = token_in_turn(&client, &session.id, 2, "cold");

    assert_eq!(under_the_machine.assigned_account, None, "it names the machine's account and is on none");
    assert_eq!(under_work, "work-token");
    assert_eq!(on(&client, &session.id), Some(work.id));
    let events = read_all(&client, &session.id);
    let expired: Vec<_> = events.iter().filter(|e| matches!(e.body, EventBody::CacheExpired { .. })).collect();
    assert!(expired.is_empty(), "{expired:?}");
}

/// An agent that's only waiting on a pull request doesn't count as running, so moving its chat doesn't ask. It's
/// started again at once under the account the chat was moved to, and goes on waiting, so it hears however soon the
/// wait ends.
#[test]
fn a_chat_only_waiting_on_a_pull_request_moves_without_asking_and_waits_on_under_its_account() {
    let mut stack = Stack::start();
    let gh = StandInGh::start(&mut stack);
    gh.say(OPEN);
    let chat = TwoAccounts::on(stack);
    let client = chat.client();
    client.session_prompt(&chat.session, WAIT, None).unwrap();
    wait_for_status(&client, &chat.session, SessionStatus::Background);

    let moved = client.session_assign_account(&chat.session, &chat.personal.id, false).unwrap();
    wait_until("the agent to start again", || launches(&read_all(&client, &chat.session)).len() == 2);
    let waiting = client.session_get(&chat.session).unwrap();
    gh.say(MERGED);
    // Another chat's read finds the pull request merged, which ends the wait.
    client.session_prompt(&chat.another_chat(&client), READ, None).unwrap();
    let woken = assistant_text(&wait_for_turn_end(&client, &chat.session, 3));
    let under = token_in_turn(&client, &chat.session, 4, "say:still here");

    assert_eq!(moved.status, SessionStatus::Background, "it was moved without asking");
    assert_eq!((waiting.status, waiting.wait.map(|wait| wait.number)), (SessionStatus::Background, Some(7)));
    assert!(woken.contains("every watched PR has merged"), "{woken}");
    assert_eq!(under, "personal-token");
    assert_eq!(launches(&read_all(&client, &chat.session)), [false, true], "started again once, at the move");
}

/// A chat's title is asked for under the login its launches take, so a chat moved to an account before its first
/// prompt is titled under that account's, and its first launch leaves it there.
#[test]
fn a_chat_moved_before_its_first_prompt_is_titled_under_its_accounts_login_and_stays_on_it() {
    let chat = TwoAccounts::start();
    let client = chat.client();
    let fresh = chat.another_chat(&client);

    client.session_assign_account(&fresh, &chat.personal.id, false).unwrap();
    client.session_prompt(&fresh, "say:titled:Under Personal", None).unwrap();
    wait_until("the chat's generated title", || {
        client.session_get(&fresh).unwrap().title.as_deref() == Some("Under Personal")
    });

    // Once its launch has been recorded, which put it on no other account.
    wait_for_turn_end(&client, &fresh, 1);

    let token = std::fs::read_to_string(chat.stack.agent_state().join("title-token")).unwrap();
    assert_eq!(token, "personal-token");
    assert_eq!(on(&client, &fresh), Some(chat.personal.id));
}
