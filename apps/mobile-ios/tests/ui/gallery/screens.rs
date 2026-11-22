//! The screens the phone's gallery draws: what the hub holds for each, from `worlds.rs`, and what to do in the app
//! before its picture is taken.

#![cfg(test)]

use client_core::fake_hub::World;

use super::worlds;

pub(super) struct Screen {
    /// Also the image's file name, in `docs/spec/screens/phone/`.
    pub(super) name: &'static str,
    /// The spec's file for the area it's about, in `docs/spec/`, which shows it beside the rules it goes with.
    pub(super) spec: &'static str,
    pub(super) hub: Hub,
    pub(super) steps: &'static [Step],
}

/// The hub a screen's phone has, if any.
pub(super) enum Hub {
    /// The phone has never paired.
    Unpaired,
    /// Paired with a hub that holds this.
    Holds(fn() -> World),
    /// Paired with a hub that takes the connection and says nothing more, so the phone stays connecting.
    NeverAnswers,
    /// Paired with a hub the phone can't reach, as off its network.
    Unreachable,
}

/// Something to do in the app once it has opened, each done once the one before it has finished.
pub(super) enum Step {
    /// Taps something, by its ID, once it's drawn.
    Tap(&'static str),
    /// Taps a workspace's row, by its ID, and waits for its chat to load.
    OpenChat(&'static str),
    /// Taps the Open on a subagent's line in the chat, by its ID, and waits for the subagent's work to load.
    OpenSubagent(&'static str),
    /// Types this where focus is, as the on-screen keyboard does.
    Type(&'static str),
    /// Opens a pairing link, as the Camera app does after scanning a hub's QR code.
    OpenLink(fn() -> String),
    /// Waits for the element with this ID to be drawn.
    WaitFor(&'static str),
    /// Holds a finger on something, by its ID, until its menu opens, then lets go.
    PressLong(&'static str),
    /// Holds a finger on the first thing drawn whose ID is this name and a number, such as a code block, which the
    /// kit numbers by where it starts in its text, until its menu opens, then lets go.
    PressLongNumbered(&'static str),
    /// Taps the row with this label in the menu that's open.
    Choose(&'static str),
    /// Taps the first element whose ID starts with this, for one a screen can't know the whole ID of, such as a part
    /// of the transcript, whose ID ends with its item's.
    TapStarting(&'static str),
    /// Has the hub stop being reachable, and the app find that out.
    LoseTheHub,
}

const SPEC: &str = "mobile.md";

pub(super) const SCREENS: &[Screen] = &[
    Screen { name: "pairing", spec: SPEC, hub: Hub::Unpaired, steps: &[] },
    Screen {
        name: "pairing-failed",
        spec: SPEC,
        hub: Hub::Unpaired,
        steps: &[Step::Tap("pairing-link"), Step::Type("roundhouse://pair"), Step::Tap("pair")],
    },
    Screen {
        name: "pairing-another",
        spec: SPEC,
        hub: Hub::Holds(worlds::hub),
        steps: &[Step::OpenLink(worlds::link_to_another_hub), Step::WaitFor("confirm-pair")],
    },
    Screen { name: "workspaces", spec: SPEC, hub: Hub::Holds(worlds::hub), steps: &[] },
    Screen { name: "workspaces-none", spec: SPEC, hub: Hub::Holds(worlds::empty), steps: &[] },
    Screen { name: "connecting", spec: SPEC, hub: Hub::NeverAnswers, steps: &[] },
    Screen { name: "unreachable", spec: SPEC, hub: Hub::Unreachable, steps: &[] },
    Screen { name: "hub-lost", spec: SPEC, hub: Hub::Holds(worlds::hub), steps: &[Step::LoseTheHub] },
    Screen { name: "chat", spec: SPEC, hub: Hub::Holds(worlds::hub), steps: &[Step::OpenChat(worlds::FOCUS_ROW)] },
    Screen {
        name: "chat-typing",
        spec: SPEC,
        hub: Hub::Holds(worlds::hub),
        steps: &[
            Step::OpenChat(worlds::FOCUS_ROW),
            Step::Tap("composer"),
            Step::Type("And show it in Home too, in the same colours."),
        ],
    },
    Screen {
        name: "chat-working",
        spec: SPEC,
        hub: Hub::Holds(worlds::working),
        steps: &[Step::OpenChat(worlds::FOCUS_ROW)],
    },
    Screen {
        name: "chat-steering",
        spec: SPEC,
        hub: Hub::Holds(worlds::working),
        steps: &[
            Step::OpenChat(worlds::FOCUS_ROW),
            Step::Tap("composer"),
            Step::Type("Keep the old colours for the dark theme."),
        ],
    },
    Screen {
        name: "question",
        spec: SPEC,
        hub: Hub::Holds(worlds::asking),
        steps: &[Step::OpenChat(worlds::FOCUS_ROW)],
    },
    Screen {
        name: "tool-use",
        spec: SPEC,
        hub: Hub::Holds(worlds::asking_to_run),
        steps: &[Step::OpenChat(worlds::FOCUS_ROW)],
    },
    Screen { name: "new-workspace", spec: SPEC, hub: Hub::Holds(worlds::hub), steps: &[Step::Tap("new-workspace")] },
    Screen {
        name: "turn-card",
        spec: SPEC,
        hub: Hub::Holds(worlds::hub),
        steps: &[Step::OpenChat(worlds::FOCUS_ROW), Step::TapStarting("turn-end/")],
    },
    Screen {
        name: "workspace-menu",
        spec: SPEC,
        hub: Hub::Holds(worlds::hub),
        steps: &[Step::PressLong(worlds::FOCUS_ROW)],
    },
    Screen {
        name: "archive-asks",
        spec: SPEC,
        hub: Hub::Holds(worlds::hub),
        steps: &[Step::PressLong(worlds::FOCUS_ROW), Step::Choose("Archive"), Step::WaitFor("action-sheet")],
    },
    Screen {
        name: "renaming",
        spec: SPEC,
        hub: Hub::Holds(worlds::hub),
        steps: &[Step::PressLong(worlds::FOCUS_ROW), Step::Choose("Rename…"), Step::WaitFor("renaming")],
    },
    Screen {
        name: "subagent",
        spec: SPEC,
        hub: Hub::Holds(worlds::subagents),
        steps: &[Step::OpenChat(worlds::FOCUS_ROW), Step::OpenSubagent("open-subagent/toolu-1")],
    },
    Screen {
        name: "proposal",
        spec: SPEC,
        hub: Hub::Holds(worlds::proposing),
        steps: &[
            Step::OpenChat(worlds::FOCUS_ROW),
            Step::Tap("open-proposal/8d0c6a5e-proposal"),
            Step::WaitFor("proposal-note"),
        ],
    },
    Screen {
        name: "model-menu",
        spec: SPEC,
        hub: Hub::Holds(worlds::hub),
        steps: &[Step::OpenChat(worlds::FOCUS_ROW), Step::Tap("model-picker"), Step::WaitFor("popup-menu")],
    },
    Screen {
        name: "chats-menu",
        spec: SPEC,
        hub: Hub::Holds(worlds::several_chats),
        steps: &[Step::OpenChat(worlds::FOCUS_ROW), Step::Tap("chats"), Step::WaitFor("popup-menu")],
    },
    Screen {
        name: "copy-menu",
        spec: SPEC,
        hub: Hub::Holds(worlds::hub),
        steps: &[Step::OpenChat(worlds::FOCUS_ROW), Step::PressLongNumbered("codeblock")],
    },
    Screen { name: "hub", spec: SPEC, hub: Hub::Holds(worlds::hub), steps: &[Step::Tap("hub")] },
];
