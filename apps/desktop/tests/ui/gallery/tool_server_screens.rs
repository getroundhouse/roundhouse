//! MCP servers' screens in Settings, each of a state its rules in `settings.md` say: the servers, none yet, adding one,
//! and a row's menu.

#![cfg(test)]

use super::frame::Frame;
use super::screens::FOCUS;
use super::steps::{Screen, Step};
use super::worlds::{conversation, one_tool_server, tool_server_refused, tool_servers};
use client_core::fake_hub::World;

/// Shows MCP servers, once Settings is open.
const SERVERS: Step = Step::Click("settings-page-mcp");
/// Opens Add server's panel.
const ADD: Step = Step::Click("mcp-add");
/// Opens the panel for adding a server a key is sent to, from Add server's, with the cursor in its URL.
const KEY: Step = Step::Click("mcp-use-key");
/// Puts the cursor in a name's field.
const NAME: Step = Step::Click("mcp-name");

/// Rests the pointer on what the panel says under its title, which has no hover of its own.
const AWAY: Step = Step::Point("mcp-panel-says");

const fn mcp(name: &'static str, world: fn() -> World, steps: &'static [Step]) -> Screen {
    Screen { name, spec: "settings.md", world: Some(world), frame: Frame::Layout, steps }
}

pub(crate) const SCREENS: &[Screen] = &[
    mcp("settings-mcp", tool_servers, &[Step::Select(FOCUS), Step::OpenSettings, SERVERS]),
    mcp("settings-mcp-none", conversation, &[Step::Select(FOCUS), Step::OpenSettings, SERVERS]),
    mcp("settings-mcp-choosing", conversation, &[Step::Select(FOCUS), Step::OpenSettings, SERVERS, ADD, AWAY]),
    mcp(
        "settings-mcp-key",
        one_tool_server,
        &[
            Step::Select(FOCUS),
            Step::OpenSettings,
            SERVERS,
            ADD,
            KEY,
            Step::Type("https://mcp.northwind.example/mcp"),
            NAME,
            Step::Type("Runbooks"),
            Step::Click("mcp-key"),
            Step::Type("a-made-up-key"),
            AWAY,
        ],
    ),
    mcp(
        "settings-mcp-second",
        one_tool_server,
        &[
            Step::Select(FOCUS),
            Step::OpenSettings,
            SERVERS,
            ADD,
            KEY,
            Step::Type("https://mcp.linear.app/mcp"),
            NAME,
            Step::Type("Linear (Northwind)"),
            AWAY,
        ],
    ),
    mcp(
        "settings-mcp-refused",
        tool_server_refused,
        &[
            Step::Select(FOCUS),
            Step::OpenSettings,
            SERVERS,
            ADD,
            Step::Click("mcp-use-neither"),
            Step::Type("https://docs.northwind.example/mcp"),
            NAME,
            Step::Type("Linear"),
            Step::Click("mcp-panel-add"),
            Step::WaitFor("mcp-refused"),
            AWAY,
        ],
    ),
    mcp(
        "settings-mcp-menu",
        tool_servers,
        &[Step::Select(FOCUS), Step::OpenSettings, SERVERS, Step::Click("mcp-more-linear")],
    ),
];
