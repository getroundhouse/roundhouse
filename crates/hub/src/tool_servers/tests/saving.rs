//! Saving a server once it has been asked for its tools: what's done at once, what's asked first, and what's saved once
//! it has been asked, when something has changed meanwhile.

use protocol::client::tool_server_checks::ToolServerToCheck;
use protocol::client::tool_servers::{ToolServerAdd, ToolServerEdit};
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{ToolServerAuthEdit, ToolServerKey};

use super::super::{Kept, Saving, ServerAsSent, ToolServers, When};
use super::support::{edit, named, servers_in, with_key, with_neither};

const URL: &str = "https://mcp.example.com/mcp";
const QUERIED: &str = "https://mcp.example.com/mcp?tools=some";
const ELSEWHERE: &str = "https://eu.mcp.example.com/mcp";

/// What a save came to, and the name, the URL and the header of each server there is then.
type CameTo = (Result<&'static str, ErrorCode>, Vec<(String, String, Option<String>)>);

fn came_to(servers: &ToolServers, saved: Result<Kept, ApiError>) -> CameTo {
    let saved = match saved {
        Ok(Kept::Saved(_)) => Ok("saved"),
        Ok(Kept::LetGo) => Ok("let go"),
        Err(e) => Err(e.code),
    };
    let set = servers.sent().borrow().set();
    let each = |server: protocol::worker::ProxiedServer| {
        let header = server.header.map(|header| format!("{}: {}", header.name.as_str(), header.value.as_str()));
        (server.label.to_string(), server.url.as_str().to_string(), header)
    };
    (saved, set.servers.into_iter().map(each).collect())
}

/// What saving `of` comes to once its server has been asked what saving it asks, with `meanwhile` done between, and
/// `still` saying whether it's still wanted after.
fn asked_then(
    servers: &ToolServers,
    of: &ToolServerToCheck,
    meanwhile: impl FnOnce(&ToolServers),
    still: bool,
) -> CameTo {
    let Ok(Saving::Ask(asked)) = servers.save_or_ask(of, When::Listed, || true) else {
        panic!("there's nothing to ask")
    };
    meanwhile(servers);
    let saved = servers.save_asked(of, &asked, || still);
    came_to(servers, saved)
}

fn keyed(key: &str, given_for: &str) -> ToolServerAuthEdit {
    let (header, key) = ("Authorization".into(), Some(ToolServerKey::new(key)));
    ToolServerAuthEdit::Key { header, key, url: given_for.into() }
}

/// What another client does meanwhile.
type Meanwhile = fn(&ToolServers);

fn server(name: &str, url: &str, key: Option<&str>) -> (String, String, Option<String>) {
    (name.to_string(), url.to_string(), key.map(|key| format!("Authorization: Bearer {key}")))
}

#[test]
fn a_save_is_done_at_once_where_theres_nothing_to_ask_or_its_saved_anyway_and_otherwise_asks_first() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, _) = servers_in(dir.path());
    let add = ToolServerToCheck::Add(with_key("t", "Linear", "made-up"));

    let asks = servers.save_or_ask(&add, When::Listed, || true).unwrap();

    assert!(matches!(asks, Saving::Ask(_)), "{asks:?}");
    assert_eq!(came_to(&servers, Ok(Kept::LetGo)).1, [], "nothing until it's asked");
    for (of, when) in [(add, When::Anyway), (ToolServerToCheck::Edit(named("t", "Sentry")), When::Listed)] {
        let done = servers.save_or_ask(&of, when, || true).unwrap();
        assert!(matches!(done, Saving::Done(Kept::Saved(_))), "{of:?}: {done:?}");
    }
    assert_eq!(came_to(&servers, Ok(Kept::LetGo)).1, [server("Sentry", URL, Some("made-up"))]);
}

/// An edit of the server's query, with its key as it is, once something has changed meanwhile.
#[test]
fn an_edit_thats_been_asked_is_saved_only_as_it_was_asked_or_where_nothing_would_be_asked_now() {
    let queried = ToolServerToCheck::Edit(ToolServerEdit { url: Some(QUERIED.into()), ..edit("t") });
    let cases: [(&str, Meanwhile, bool, CameTo); 7] = [
        ("nothing", |_| {}, true, (Ok("saved"), vec![server("Linear", QUERIED, Some("old"))])),
        (
            "its name",
            |s| {
                s.edit(named("t", "Sentry")).unwrap();
            },
            true,
            (Ok("saved"), vec![server("Sentry", QUERIED, Some("old"))]),
        ),
        ("cancelled", |_| {}, false, (Ok("let go"), vec![server("Linear", URL, Some("old"))])),
        (
            "the same change",
            |s| {
                s.edit(ToolServerEdit { url: Some(QUERIED.into()), ..edit("t") }).unwrap();
            },
            true,
            (Ok("saved"), vec![server("Linear", QUERIED, Some("old"))]),
        ),
        (
            "another key",
            |s| {
                s.edit(ToolServerEdit { auth: Some(keyed("new", URL)), ..edit("t") }).unwrap();
            },
            true,
            (Err(ErrorCode::Conflict), vec![server("Linear", URL, Some("new"))]),
        ),
        (
            "where it is",
            |s| {
                let moved =
                    ToolServerEdit { url: Some(ELSEWHERE.into()), auth: Some(keyed("eu", ELSEWHERE)), ..edit("t") };
                s.edit(moved).unwrap();
            },
            true,
            (Err(ErrorCode::InvalidRequest), vec![server("Linear", ELSEWHERE, Some("eu"))]),
        ),
        (
            "removed",
            |s| {
                s.remove(&"t".into()).unwrap();
            },
            true,
            (Err(ErrorCode::NotFound), vec![]),
        ),
    ];
    for (meanwhile, change, still, expected) in cases {
        let dir = tempfile::tempdir().unwrap();
        let (servers, _) = servers_in(dir.path());
        servers.add(with_key("t", "Linear", "old")).unwrap();

        assert_eq!(asked_then(&servers, &queried, change, still), expected, "{meanwhile}");
    }
}

#[test]
fn a_server_thats_been_asked_isnt_added_where_its_name_was_taken_meanwhile_and_is_where_its_id_was_added() {
    let add = ToolServerToCheck::Add(with_neither("t", "Linear"));
    let cases: [(&str, Meanwhile, CameTo); 2] = [
        (
            "its name, by another",
            |s| {
                s.add(ToolServerAdd { url: ELSEWHERE.into(), ..with_neither("o", "linear") }).unwrap();
            },
            (Err(ErrorCode::Conflict), vec![server("linear", ELSEWHERE, None)]),
        ),
        // As a client sending it again does, and is answered with the server there is.
        (
            "its ID",
            |s| {
                s.add(with_key("t", "Linear", "made-up")).unwrap();
            },
            (Ok("saved"), vec![server("Linear", URL, Some("made-up"))]),
        ),
    ];
    for (meanwhile, change, expected) in cases {
        let dir = tempfile::tempdir().unwrap();
        let (servers, _) = servers_in(dir.path());

        assert_eq!(asked_then(&servers, &add, change, true), expected, "{meanwhile}");
    }
}

/// An edit's own `saved_anyway` is ignored, since the save's choice says.
#[test]
fn an_edit_saved_anyway_has_running_agents_list_its_tools_again_and_one_saved_once_its_listed_doesnt() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, _) = servers_in(dir.path());
    servers.add(with_key("t", "Linear", "made-up")).unwrap();
    let as_sent =
        |servers: &ToolServers| -> ServerAsSent { servers.sent().borrow().as_sent().of(&"t".into()).cloned().unwrap() };
    let another_key =
        |key| ToolServerToCheck::Edit(ToolServerEdit { auth: Some(keyed(key, URL)), saved_anyway: true, ..edit("t") });

    let before = as_sent(&servers);
    let listed = another_key("second");
    let Ok(Saving::Ask(asked)) = servers.save_or_ask(&listed, When::Listed, || true) else { panic!("nothing to ask") };
    servers.save_asked(&listed, &asked, || true).unwrap();
    let once_listed = as_sent(&servers);
    servers.save_or_ask(&another_key("third"), When::Anyway, || true).unwrap();
    let anyway = as_sent(&servers);

    assert!(once_listed != before && !once_listed.lists_other_tools_than(&before));
    assert!(anyway.lists_other_tools_than(&once_listed));
}
