//! The screens about the context and the account, beside Send: the plug's card, quiet, and while one of the hub's MCP
//! servers needs you to sign in to it, as its sign-in goes on and once it has come to nothing, then the ring's card, as
//! it usually is, when a setting caps the context and when it's the one card to say whose account it is, for a login on
//! the machine and for one the hub holds, then the limits' card, with a model's own weekly limit too, with the menu
//! that moves the chat to another account, and on an account that's using what it reserves of its week, with the line
//! above the composer that says so.

#![cfg(test)]

use look::Region;

use super::frame::{CONTEXT, Frame, Part, TOOL_SERVERS};
use super::screens::FOCUS;
use super::steps::{Screen, Step, Target};
use super::worlds::{
    chat_servers, chat_servers_to_sign_in_to, context, context_capped, context_held_login, context_in_reserve,
    context_placed_automatically, context_to_move, context_with_a_models_limit, context_without_limits,
    every_account_out, needs_an_account, tool_sign_in_refused,
};

/// The screens about the context and the account.
pub(crate) const SCREENS: &[Screen] = &[
    Screen {
        name: "mcp-servers",
        spec: "chats.md",
        world: Some(chat_servers),
        frame: TOOL_SERVERS,
        steps: &[
            Step::Select(FOCUS),
            Step::WaitFor("usage"),
            Step::Hover { on: Target::Id("mcp"), shows: Region::ToolServersCard.id() },
        ],
    },
    Screen {
        name: "mcp-servers-sign-in",
        spec: "chats.md",
        world: Some(chat_servers_to_sign_in_to),
        frame: TOOL_SERVERS,
        steps: &[
            Step::Select(FOCUS),
            Step::WaitFor("usage"),
            Step::Hover { on: Target::Id("mcp"), shows: Region::ToolServersCard.id() },
        ],
    },
    Screen {
        name: "mcp-servers-signing-in",
        spec: "chats.md",
        world: Some(chat_servers_to_sign_in_to),
        frame: TOOL_SERVERS,
        steps: &[
            Step::Select(FOCUS),
            Step::WaitFor("usage"),
            Step::Hover { on: Target::Id("mcp"), shows: Region::ToolServersCard.id() },
            Step::Click("mcp-sign-in-linear"),
            Step::Point("mcp"),
        ],
    },
    Screen {
        name: "mcp-servers-sign-in-refused",
        spec: "chats.md",
        world: Some(tool_sign_in_refused),
        frame: TOOL_SERVERS,
        steps: &[
            Step::Select(FOCUS),
            Step::Hover { on: Target::Id("mcp"), shows: Region::ToolServersCard.id() },
            Step::Click("mcp-sign-in-linear"),
            Step::WaitFor("mcp-why"),
            Step::Point("mcp"),
        ],
    },
    Screen {
        name: "context",
        spec: "chats.md",
        world: Some(context),
        frame: CONTEXT,
        // The bars come once the hub has said what's left of the limits, which the picture is taken with.
        steps: &[
            Step::Select(FOCUS),
            Step::WaitFor("usage"),
            Step::Hover { on: Target::Id("context"), shows: Region::ContextCard.id() },
        ],
    },
    Screen {
        name: "context-capped",
        spec: "chats.md",
        world: Some(context_capped),
        frame: CONTEXT,
        steps: &[
            Step::Select(FOCUS),
            Step::WaitFor("usage"),
            Step::Hover { on: Target::Id("context"), shows: Region::ContextCard.id() },
        ],
    },
    Screen {
        name: "context-account",
        spec: "chats.md",
        world: Some(context_without_limits),
        // What the hover waits for is the bottom of the context card, so the frame names the whole card.
        frame: CONTEXT,
        steps: &[Step::Select(FOCUS), Step::Hover { on: Target::Id("context"), shows: "account" }],
    },
    Screen {
        name: "context-held-login",
        spec: "chats.md",
        world: Some(context_held_login),
        frame: CONTEXT,
        steps: &[Step::Select(FOCUS), Step::Hover { on: Target::Id("context"), shows: "account" }],
    },
    Screen {
        name: "usage",
        spec: "chats.md",
        world: Some(context),
        frame: Frame::Around(Part::Region(Region::Composer), &[Part::Region(Region::UsageCard)]),
        steps: &[
            Step::Select(FOCUS),
            Step::WaitFor("usage"),
            Step::Hover { on: Target::Id("usage"), shows: Region::UsageCard.id() },
        ],
    },
    Screen {
        name: "usage-model-limit",
        spec: "chats.md",
        world: Some(context_with_a_models_limit),
        frame: Frame::Around(Part::Region(Region::Composer), &[Part::Region(Region::UsageCard)]),
        steps: &[
            Step::Select(FOCUS),
            Step::WaitFor("usage"),
            Step::Hover { on: Target::Id("usage"), shows: Region::UsageCard.id() },
        ],
    },
    Screen {
        name: "usage-move",
        spec: "chats.md",
        world: Some(context_to_move),
        frame: Frame::Around(Part::Region(Region::Composer), &[Part::Region(Region::UsageCard)]),
        steps: &[
            Step::Select(FOCUS),
            Step::WaitFor("usage"),
            Step::Hover { on: Target::Id("usage"), shows: Region::UsageCard.id() },
        ],
    },
    Screen {
        name: "usage-move-menu",
        spec: "chats.md",
        world: Some(context_to_move),
        frame: Frame::Around(Part::Region(Region::Composer), &[Part::Region(Region::UsageCard), Part::Menu]),
        steps: &[
            Step::Select(FOCUS),
            Step::WaitFor("usage"),
            Step::Hover { on: Target::Id("usage"), shows: Region::UsageCard.id() },
            Step::Click("account-move"),
        ],
    },
    Screen {
        name: "usage-chosen-automatically",
        spec: "chats.md",
        world: Some(context_placed_automatically),
        frame: Frame::Around(Part::Region(Region::Composer), &[Part::Region(Region::UsageCard)]),
        steps: &[
            Step::Select(FOCUS),
            Step::WaitFor("usage"),
            Step::Hover { on: Target::Id("usage"), shows: Region::UsageCard.id() },
        ],
    },
    Screen {
        name: "usage-in-reserve",
        spec: "chats.md",
        world: Some(context_in_reserve),
        frame: Frame::Around(
            Part::Region(Region::ReserveLine),
            &[Part::Region(Region::Composer), Part::Region(Region::UsageCard)],
        ),
        steps: &[
            Step::Select(FOCUS),
            Step::WaitFor("usage"),
            Step::Hover { on: Target::Id("usage"), shows: Region::UsageCard.id() },
        ],
    },
    Screen {
        name: "reserve-line",
        spec: "chats.md",
        world: Some(context_in_reserve),
        frame: Frame::Around(Part::Region(Region::ReserveLine), &[Part::Region(Region::Composer), Part::Menu]),
        steps: &[Step::Select(FOCUS), Step::WaitFor("usage"), Step::Click("reserve-move")],
    },
    Screen {
        name: "chat-needs-an-account",
        spec: "chats.md",
        world: Some(needs_an_account),
        frame: Frame::Around(Part::Region(Region::AccountWait), &[Part::Region(Region::Composer)]),
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "chat-needs-an-account-every-one-out",
        spec: "chats.md",
        world: Some(every_account_out),
        frame: Frame::Around(Part::Region(Region::AccountWait), &[Part::Region(Region::Composer)]),
        steps: &[Step::Select(FOCUS)],
    },
];
