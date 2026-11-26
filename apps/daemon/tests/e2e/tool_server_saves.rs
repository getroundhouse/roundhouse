//! Adding a tool server with `tool_server_save`: a running hub asks it for its tools with the key it's to have, and
//! adds it once it lists them, or once the person has it added anyway. The server is a stand-in in the test's process.

#![cfg(test)]

use protocol::client::tool_server_checks::{NoToolList, ToolServerToCheck};
use protocol::client::tool_server_saves::{SaveWhen, ToolServerSave, ToolServerSaved};
use protocol::client::tool_servers::ToolServerAdd;
use protocol::model::{ToolServerAuthGiven, ToolServerKey};

use crate::support::Stack;
use crate::tool_servers::{KEY, stand_in};

const WRONG: &str = "made-up-wrong-key-of-a-person";

/// A key the server refuses, then that key added anyway, then the one it takes.
#[test]
fn a_server_is_added_once_it_lists_its_tools_with_its_key_and_one_that_doesnt_only_anyway() {
    let (url, heard) = stand_in();
    let stack = Stack::start();
    let client = stack.client();
    let save = |id: &str, key: &str, when| {
        let auth = ToolServerAuthGiven::Key { header: "X-Key".into(), key: ToolServerKey::new(key) };
        let of = ToolServerToCheck::Add(ToolServerAdd { id: id.into(), name: id.into(), url: url.clone(), auth });
        client.call(ToolServerSave { id: format!("save-{id}").into(), of, when }).unwrap()
    };
    let added = || client.records().unwrap().tool_servers.into_iter().map(|server| server.id).collect::<Vec<_>>();

    let refused = save("Refused", WRONG, SaveWhen::Listed);
    assert_eq!(refused, ToolServerSaved::NotListed { why: NoToolList::KeyRefused { status: 401 } });
    assert_eq!(added(), []);
    assert!(matches!(save("Refused", WRONG, SaveWhen::Anyway), ToolServerSaved::Saved { .. }));

    let listed = save("Linear", KEY, SaveWhen::Listed);

    let ToolServerSaved::Saved { server } = listed else { panic!("{listed:?}") };
    assert_eq!(server.id, "Linear".into());
    assert!(heard.try_iter().any(|took| took.method == "tools/list" && took.key == KEY), "with its key");
    assert_eq!(added().len(), 2);
}
