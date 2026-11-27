//! Which login a chat is on, the hub's for an account or the ambient one, and moving it to another account the hub
//! holds a login for, through a real hub, on a machine with no login of its own, so every launch runs under a login
//! the hub hands out. Which account a chat is on, and what a move does, are the hub's, and its own tests in
//! `hub/logins/tests/` prove them on stand-ins for its workers. These are what spans processes: a restart of the hub,
//! the login reaching the agent's process, and a wait on a pull request through the hub's tools.

#![cfg(test)]

use client_core::Connection;
use protocol::capability::settings::Settings;
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{Account, AgentKind, LoginToken, SessionId, SessionStatus};

use crate::accounts::log_out;
use hub::GithubPullRequestState::{Merged, Open};

use crate::support::github::{StandInGithub, wait_for_address};
use crate::support::{Stack, assistant_text, read_all, wait_for_status, wait_for_turn_end, wait_until};

const TOKEN: &str = "env:CLAUDE_CODE_OAUTH_TOKEN";
const WAIT: &str = r#"mcp:wait_for_pull_requests {"pull_requests":[7],"until":"merged"}"#;
const READ: &str = r#"mcp:get_pull_requests {"pull_requests":[7]}"#;

/// A stack whose machine has no login, with two accounts the hub holds, Work, which is the default as the first added,
/// and Personal, and a chat that has had a turn under Work, which put it on Work.
struct TwoAccounts {
    stack: Stack,
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
        connect(&client, "Work");
        let personal = connect(&client, "Personal");
        let (_, session) = stack.workspace(&client);
        assert_eq!(token_in_turn(&client, &session.id, 1, "say:hello"), "work-token");
        TwoAccounts { stack, personal, session: session.id }
    }

    fn client(&self) -> Connection {
        self.stack.client()
    }

    /// Another chat in the same workspace, on no account until its agent first starts.
    fn another_chat(&self, client: &Connection) -> SessionId {
        let workspace = client.session_get(&self.session).unwrap().workspace_id;
        client.session_create(&workspace, AgentKind::Claude, Settings::default(), None).unwrap().id
    }
}

/// Has the hub hold a login for Claude called `name`, whose token is its name in lower case, then `-token`.
fn connect(client: &Connection, name: &str) -> Account {
    let token = LoginToken::new(format!("{}-token", name.to_lowercase()));
    client.account_connect(AgentKind::Claude, name, token, None).unwrap()
}

/// The token the agent runs under, as it says once it has done `then` in the `turn`th turn.
fn token_in_turn(client: &Connection, session: &SessionId, turn: usize, then: &str) -> String {
    client.session_prompt(session, &format!("{then};{TOKEN}"), None).unwrap();
    let text = assistant_text(&wait_for_turn_end(client, session, turn));
    text.rsplit("CLAUDE_CODE_OAUTH_TOKEN=").next().unwrap().to_string()
}

/// Whether each launch resumed the conversation, in order.
fn launches(events: &[SessionEvent]) -> Vec<bool> {
    let resumed = |e: &SessionEvent| match e.body {
        EventBody::AgentStarted { resumed, .. } => Some(resumed),
        _ => None,
    };
    events.iter().filter_map(resumed).collect()
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

/// An agent that's only waiting on a pull request doesn't count as running, so moving its chat doesn't ask. It's
/// started again at once under the account the chat was moved to, and goes on waiting, so it hears however soon the
/// wait ends.
#[test]
fn a_chat_only_waiting_on_a_pull_request_moves_without_asking_and_waits_on_under_its_account() {
    let github = StandInGithub::start();
    github.say(7, Open);
    let (stack, _gh) = github.stack();
    let chat = TwoAccounts::on(stack);
    let client = chat.client();
    wait_for_address(&client);
    client.session_prompt(&chat.session, WAIT, None).unwrap();
    wait_for_status(&client, &chat.session, SessionStatus::Background);

    let moved = client.session_assign_account(&chat.session, &chat.personal.id, false).unwrap();
    wait_until("the agent to start again", || launches(&read_all(&client, &chat.session)).len() == 2);
    let waiting = client.session_get(&chat.session).unwrap();
    github.say(7, Merged);
    // Another chat's read finds the pull request merged, which ends the wait.
    client.session_prompt(&chat.another_chat(&client), READ, None).unwrap();
    let woken = assistant_text(&wait_for_turn_end(&client, &chat.session, 3));
    let under = token_in_turn(&client, &chat.session, 4, "say:still here");

    assert_eq!(moved.status, SessionStatus::Background, "it was moved without asking");
    assert_eq!((waiting.status, waiting.waits.first().map(|wait| wait.number)), (SessionStatus::Background, Some(7)));
    assert!(woken.contains("every watched PR has merged"), "{woken}");
    assert_eq!(under, "personal-token");
    assert_eq!(launches(&read_all(&client, &chat.session)), [false, true], "started again once, at the move");
}
