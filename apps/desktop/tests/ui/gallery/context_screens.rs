//! The screens about the context and the account, beside Send: the ring's card, as it usually is, when a setting caps
//! the context and when it's the one card to say whose account it is, then the limits' card, and with a model's own
//! weekly limit too.

#![cfg(test)]

use look::Region;

use super::frame::{CONTEXT, Frame, Part};
use super::screens::FOCUS;
use super::steps::{Screen, Step, Target};
use super::worlds::{context, context_capped, context_with_a_models_limit, context_without_limits};

/// The screens about the context and the account.
pub(crate) const SCREENS: &[Screen] = &[
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
];
