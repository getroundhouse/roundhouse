//! Checking a tool server as it's added and edited: a running hub asks it for its tools with what workers would send
//! it, and nothing it's asked with is kept. The server is a stand-in in the test's process.

#![cfg(test)]

use std::path::PathBuf;

use protocol::client::tool_server_checks::{NoToolList, ToolServerChecked, ToolServerToCheck};
use protocol::client::tool_servers::{ToolServerAdd, ToolServerEdit};
use protocol::model::{ToolServerAuthEdit, ToolServerAuthGiven, ToolServerKey};

use crate::support::{Stack, calls};
use crate::tool_servers::{ANOTHER, CALL, KEY, add, holding, stand_in};

const WRONG: &str = "made-up-wrong-key-of-a-person";
const REFUSED: ToolServerChecked = ToolServerChecked::NotListed { why: NoToolList::KeyRefused { status: 401 } };
const LISTED: ToolServerChecked = ToolServerChecked::Listed { tools: 1 };

#[test]
fn a_server_is_asked_with_the_key_its_to_have_and_whats_checked_is_neither_kept_nor_added() {
    let (url, heard) = stand_in();
    let stack = Stack::start();
    let client = stack.client();
    let to_add = |key: &str| {
        let auth = ToolServerAuthGiven::Key { header: "X-Key".into(), key: ToolServerKey::new(key) };
        ToolServerToCheck::Add(ToolServerAdd { id: "t".into(), name: "Linear".into(), url: url.clone(), auth })
    };
    let to_edit = |key: Option<&str>| {
        let key = key.map(ToolServerKey::new);
        let auth = ToolServerAuthEdit::Key { header: "X-Key".into(), key, url: url.clone() };
        let queried = Some(format!("{url}?tools=a"));
        ToolServerToCheck::Edit(ToolServerEdit { id: "t".into(), name: None, url: queried, auth: Some(auth) })
    };

    assert_eq!(client.tool_server_check(to_add(WRONG)).unwrap(), REFUSED);
    assert_eq!(client.tool_server_check(to_add(KEY)).unwrap(), LISTED);

    assert!(heard.try_iter().all(|took| took.key == KEY));
    assert_eq!(client.records().unwrap().tool_servers, []);
    assert_eq!(holding(&stack.path(""), KEY), Vec::<PathBuf>::new());

    add(&client, url.clone());
    assert_eq!(client.tool_server_check(to_edit(Some(WRONG))).unwrap(), REFUSED);
    assert_eq!(client.tool_server_check(to_edit(Some(ANOTHER))).unwrap(), LISTED);
    assert!(heard.try_iter().all(|took| took.key == ANOTHER));
    assert_eq!(client.tool_server_check(to_edit(None)).unwrap(), LISTED, "with the one it keeps");

    let (_, chat) = stack.workspace(&client);
    assert_eq!(calls::run(&client, &chat.id, CALL, 1), [("Found issue 7".to_string(), false)]);
    assert!(heard.try_iter().all(|took| took.key == KEY), "which is what its agents have");
    for checked in [WRONG, ANOTHER] {
        assert_eq!(holding(&stack.path(""), checked), Vec::<PathBuf>::new());
    }
}
