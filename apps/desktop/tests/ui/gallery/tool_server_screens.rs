//! MCP servers' screens in Settings, each of a state its rules in `settings.md` say: the servers, none yet, adding one
//! each way, signing in to one, and a row's menu.

#![cfg(test)]

use super::frame::Frame;
use super::screens::FOCUS;
use super::steps::{Screen, Step};
use super::worlds::{
    chat_servers_wanting_more_access, conversation, no_tool_sign_in, one_tool_server, open_tool_server,
    tool_server_refused, tool_servers, tool_sign_in_refused, tool_sign_in_waiting,
};
use client_core::fake_hub::World;

/// Shows MCP servers, once Settings is open.
const SERVERS: Step = Step::Click("settings-page-mcp");
/// Opens Add server's panel, with the cursor in its name.
const ADD: Step = Step::Click("mcp-add");
/// Puts the cursor in the URL's field.
const URL: Step = Step::Click("mcp-url");
/// Opens the dropdown of the ways a server can know the person.
const WAYS: Step = Step::Click("mcp-way");
/// Adds what a panel's fields say, or starts signing in to it.
const GO: Step = Step::Click("mcp-panel-add");

/// Rests the pointer on what the panel says under its title, which has no hover of its own.
const AWAY: Step = Step::Point("mcp-panel-says");

const fn mcp(name: &'static str, world: fn() -> World, steps: &'static [Step]) -> Screen {
    Screen { name, spec: "settings.md", world: Some(world), frame: Frame::Layout, steps }
}

pub(crate) const SCREENS: &[Screen] = &[
    mcp("settings-mcp", tool_servers, &[Step::Select(FOCUS), Step::OpenSettings, SERVERS]),
    mcp(
        "settings-mcp-more-access",
        chat_servers_wanting_more_access,
        &[Step::Select(FOCUS), Step::OpenSettings, SERVERS],
    ),
    mcp("settings-mcp-none", conversation, &[Step::Select(FOCUS), Step::OpenSettings, SERVERS]),
    mcp(
        "settings-mcp-authentication",
        one_tool_server,
        &[
            Step::Select(FOCUS),
            Step::OpenSettings,
            SERVERS,
            ADD,
            Step::Type("Notion"),
            URL,
            Step::Type("https://mcp.notion.example/mcp"),
            WAYS,
        ],
    ),
    mcp(
        "settings-mcp-key",
        one_tool_server,
        &[
            Step::Select(FOCUS),
            Step::OpenSettings,
            SERVERS,
            ADD,
            Step::Type("Runbooks"),
            URL,
            Step::Type("https://mcp.northwind.example/mcp"),
            WAYS,
            Step::ClickItem("Key, Sent with every request"),
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
            Step::Type("Linear (Northwind)"),
            URL,
            Step::Type("https://mcp.linear.app/mcp"),
            AWAY,
        ],
    ),
    mcp(
        "settings-mcp-sign-in",
        one_tool_server,
        &[
            Step::Select(FOCUS),
            Step::OpenSettings,
            SERVERS,
            ADD,
            Step::Type("Notion"),
            URL,
            Step::Type("https://mcp.notion.example/mcp"),
            AWAY,
        ],
    ),
    mcp(
        "settings-mcp-waiting",
        tool_sign_in_waiting,
        &[
            Step::Select(FOCUS),
            Step::OpenSettings,
            SERVERS,
            ADD,
            Step::Type("Notion"),
            URL,
            Step::Type("https://mcp.notion.example/mcp"),
            GO,
            Step::WaitFor("mcp-open-page"),
            AWAY,
        ],
    ),
    mcp(
        "settings-mcp-open-server",
        open_tool_server,
        &[
            Step::Select(FOCUS),
            Step::OpenSettings,
            SERVERS,
            ADD,
            Step::Type("Docs"),
            URL,
            Step::Type("https://docs.northwind.example/mcp"),
            GO,
            Step::WaitFor("mcp-refused"),
            AWAY,
        ],
    ),
    mcp(
        "settings-mcp-cant-sign-in",
        no_tool_sign_in,
        &[
            Step::Select(FOCUS),
            Step::OpenSettings,
            SERVERS,
            ADD,
            Step::Type("Runbooks"),
            URL,
            Step::Type("https://mcp.northwind.example/mcp"),
            GO,
            Step::WaitFor("mcp-refused"),
            AWAY,
        ],
    ),
    mcp(
        "settings-mcp-sign-in-refused",
        tool_sign_in_refused,
        &[
            Step::Select(FOCUS),
            Step::OpenSettings,
            SERVERS,
            ADD,
            Step::Type("Linear (Ada)"),
            URL,
            Step::Type("https://mcp.linear.app/mcp"),
            GO,
            Step::WaitFor("mcp-refused"),
            AWAY,
        ],
    ),
    mcp(
        "settings-mcp-again-refused",
        tool_sign_in_refused,
        &[
            Step::Select(FOCUS),
            Step::OpenSettings,
            SERVERS,
            Step::Click("mcp-more-linear"),
            Step::ClickItem("Sign in again"),
            Step::WaitFor("mcp-refused"),
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
            Step::Type("Linear"),
            URL,
            Step::Type("https://docs.northwind.example/mcp"),
            WAYS,
            Step::ClickItem("None, Anyone can use it"),
            GO,
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
