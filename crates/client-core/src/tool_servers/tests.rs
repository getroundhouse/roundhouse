use protocol::fixtures;
use protocol::model::{ToolServer, ToolServerName};

use super::ToolServerNames;

fn named(id: &str, name: &str) -> ToolServer {
    ToolServer { name: ToolServerName::stored(name.into()), ..fixtures::tool_server(id) }
}

#[test]
fn a_server_is_shown_by_the_persons_name_for_it_as_the_hub_has_it_now() {
    let names = ToolServerNames::of(&[named("t-1", "Linear (Northwind)"), named("t-2", "Docs")]);
    let shown = |server: &str| names.shown(&server.into()).to_string();

    assert_eq!(shown("Linear_Northwind"), "Linear (Northwind)");
    assert_eq!(shown("Docs"), "Docs");
    assert_eq!(shown("linear_northwind"), "linear_northwind", "another, since agents are given its case");
    assert_eq!(shown("github"), "github");
}
