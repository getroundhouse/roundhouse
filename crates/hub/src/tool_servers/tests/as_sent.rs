//! What changes a server as an agent has to have it, whichever way it's changed, and what leaves it as it was.

use protocol::client::tool_servers::ToolServerEdit;
use protocol::model::{ToolServerAuthEdit, ToolServerKey};
use tool_sign_in::{OauthError, Renewal};

use super::super::{ServerAsSent, ToolServers};
use super::sign_ins::{GIVEN_AT_MS, LASTS, again, edited_by_signing_in, renewed, signed_in};
use super::support::{edit, holding, named, servers_in, with_key, with_neither};
use crate::store::later_ms;

const URL: &str = "https://mcp.example.com/mcp";
const QUERIED: &str = "https://mcp.example.com/mcp?tools=some";
const ELSEWHERE: &str = "https://eu.mcp.example.com/mcp";

fn as_sent(servers: &ToolServers) -> Option<ServerAsSent> {
    servers.sent().borrow().as_sent().of(&"t".into()).cloned()
}

fn at(url: &str) -> ToolServerEdit {
    ToolServerEdit { url: Some(url.into()), ..edit("t") }
}

fn keyed(header: &str, key: Option<&str>, url: &str) -> ToolServerEdit {
    let auth = ToolServerAuthEdit::Key { header: header.into(), key: key.map(ToolServerKey::new), url: url.into() };
    ToolServerEdit { auth: Some(auth), ..edit("t") }
}

fn neither() -> ToolServerEdit {
    ToolServerEdit { auth: Some(ToolServerAuthEdit::Neither), ..edit("t") }
}

fn anyway(edit: ToolServerEdit) -> ToolServerEdit {
    ToolServerEdit { saved_anyway: true, ..edit }
}

/// Each edit after the one before it: whether it leaves the server as it was, and, where it's at the same place, whether
/// an agent connected to it has its tools to list again.
#[test]
fn an_edit_changes_a_server_but_where_it_leaves_it_as_it_was_and_its_tools_by_its_query_or_saved_anyway() {
    const SAME: (bool, bool) = (false, false);
    const CALLS: (bool, bool) = (true, false);
    const TOOLS: (bool, bool) = (true, true);
    let dir = tempfile::tempdir().unwrap();
    let (servers, _) = servers_in(dir.path());
    servers.add(with_key("t", "Linear", "made-up")).unwrap();
    servers.add(with_neither("n", "Docs")).unwrap();
    let edits = [
        ("its name", named("t", "Sentry"), SAME),
        ("its name, saved anyway", anyway(named("t", "Linear (EU)")), SAME),
        ("the key it has", keyed("Authorization", Some("made-up"), URL), SAME),
        ("the URL it has, saved anyway", anyway(at(URL)), SAME),
        ("the URL it has, spelled otherwise", at("HTTPS://MCP.example.com:443/mcp"), SAME),
        ("another's name", named("n", "Notion"), SAME),
        ("another key", keyed("Authorization", Some("another"), URL), CALLS),
        ("its key in another header", keyed("X-Key", None, URL), CALLS),
        ("no key", neither(), CALLS),
        ("a key again", keyed("X-Key", Some("another"), URL), CALLS),
        ("another key, saved anyway", anyway(keyed("X-Key", Some("third"), URL)), TOOLS),
        ("the same again, saved anyway", anyway(keyed("X-Key", Some("third"), URL)), SAME),
        ("another key, after that", keyed("X-Key", Some("fourth"), URL), CALLS),
        ("its query", at(QUERIED), TOOLS),
        ("its query, saved anyway", anyway(at(URL)), TOOLS),
        (
            "where it is",
            ToolServerEdit { url: Some(ELSEWHERE.into()), ..keyed("X-Key", Some("fifth"), ELSEWHERE) },
            CALLS,
        ),
    ];
    for (edited, edit, (changes, lists)) in edits {
        let before = as_sent(&servers).unwrap();

        servers.edit(edit).unwrap();

        let now = as_sent(&servers).unwrap();
        assert_eq!((now != before, now.lists_other_tools_than(&before)), (changes, lists), "{edited}");
        assert_eq!(now.place() != before.place(), edited == "where it is", "{edited}");
    }
    assert!(!format!("{:?}", servers.sent().borrow().as_sent()).contains("fifth"), "nor any key");
    assert_eq!(holding(dir.path(), "fifth"), ["tool-servers"]);
}

/// So a server that's refused as it's edited is as it was, however that was saved.
#[test]
fn an_edit_thats_refused_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, _) = servers_in(dir.path());
    servers.add(with_key("t", "Linear", "made-up")).unwrap();
    let before = as_sent(&servers);

    servers.edit(anyway(at(ELSEWHERE))).unwrap_err();

    assert_eq!(as_sent(&servers), before);
}

#[test]
fn a_sign_in_changes_a_server_and_renewing_it_doesnt_and_one_that_needs_it_is_nothing_to_connect_to() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, _) = servers_in(dir.path());
    let due = |now_ms| servers.check_sign_ins(now_ms).unwrap().due.remove(0);
    signed_in(&servers, "t", "Linear");
    let first = as_sent(&servers).unwrap();
    let ends_at_ms = later_ms(GIVEN_AT_MS, LASTS);

    renewed(&servers);
    assert_eq!(as_sent(&servers).as_ref(), Some(&first));
    servers.edit(anyway(at(QUERIED))).unwrap();
    let queried = as_sent(&servers).unwrap();
    let refused = Renewal::SignInAgain(OauthError::new("invalid_grant"));
    let ends_again_at_ms = later_ms(ends_at_ms, LASTS);
    servers.renewed(&due(ends_again_at_ms), refused, ends_again_at_ms).unwrap();
    assert_eq!(as_sent(&servers), None);

    again(&servers, "t").unwrap();
    let signed_in_again = as_sent(&servers).unwrap();
    assert_ne!(signed_in_again, queried);
    assert!(!signed_in_again.lists_other_tools_than(&queried), "what was saved anyway before isn't again");
    edited_by_signing_in(&servers, "t", URL);
    let edited = as_sent(&servers).unwrap();
    assert!(edited.lists_other_tools_than(&signed_in_again), "by its query");
}

/// Under the same ID, with the same key at the same URL.
#[test]
fn a_server_removed_and_added_again_is_another() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, _) = servers_in(dir.path());
    servers.add(with_key("t", "Linear", "made-up")).unwrap();
    let before = as_sent(&servers).unwrap();

    servers.remove(&"t".into()).unwrap();
    assert_eq!(as_sent(&servers), None);
    servers.add(with_key("t", "Linear", "made-up")).unwrap();

    assert_ne!(as_sent(&servers), Some(before));
}
