//! Chats whose replies point to images in the worktree, for the screens about them: a board of signals that showed the
//! wrong light for a signal at stop, and a setting shown off and on, and among words. One whose reply draws a diagram,
//! and one whose reply points to a recording.

#![cfg(test)]

use protocol::attachment::{Attachment, MediaType};
use protocol::event::LinkedImage;
use protocol::model::{AttachmentId, SessionStatus};

use super::pictures;
use super::worlds::chat_world;
use client_core::fake_hub::World;
use client_core::fake_hub::events::{first_turn, replied, text_pointing, user_prompt};

/// Where the reply in [`pointed`] says its screenshot is.
const AFTER: &str = "docs/screens/board-after.png";

/// The screenshot in [`pointed`]'s reply, as it's drawn, which is known by where it starts in the reply.
pub(super) const AFTER_DRAWN: &str = "linked-image/91";

/// The diagram in [`diagrammed`]'s reply, as it's drawn, which is known by where it starts in the reply.
pub(super) const DIAGRAM_DRAWN: &str = "diagram/44";

/// The recording in [`pointed_at_a_video`]'s reply, as it's drawn, which is known by where it starts in the reply.
pub(super) const VIDEO_DRAWN: &str = "linked-image/91";

/// Where the reply in [`pointed_at_a_video`] says its recording is.
const RECORDING: &str = ".context/recordings/setting-up.webm";

/// Where the replies in [`pointed_on_lines`] and [`pointed_among_words`] say their screenshots are.
const OFF: &str = "docs/screens/wake-off.png";
const ON: &str = "docs/screens/wake-on.png";

/// The agent fixing the board, and pointing to its screenshot of it after, in the worktree, which the hub kept.
pub(super) fn pointed() -> World {
    let reply = format!(
        "The board reads each signal's own aspect now, so the fourth shows red while it's at stop.\n\n\
         ![The board after the fix]({AFTER})\n\nThe screenshot is in `docs/screens/`, beside the design."
    );
    let drawn = reply.find("![").map(|starts| format!("linked-image/{starts}"));
    assert_eq!(drawn.as_deref(), Some(AFTER_DRAWN), "the screenshot is known by where it starts in the reply");
    let prompt = "Fix the board so a signal at stop shows red, and show me how it looks after.";
    pointing(prompt, &reply, vec![(AFTER, MediaType::Png, pictures::signal_board())])
}

/// The agent showing a workspace setting up, and pointing to its recording of it, which the hub kept.
pub(super) fn pointed_at_a_video() -> World {
    let reply = format!(
        "Setup's progress shows in the chat now, as each step starts and ends. Here it is running:\n\n\
         ![A workspace setting up]({RECORDING})\n\nThe recording is in `.context/recordings/`."
    );
    let drawn = reply.find("![").map(|starts| format!("linked-image/{starts}"));
    assert_eq!(drawn.as_deref(), Some(VIDEO_DRAWN), "the recording is known by where it starts in the reply");
    let prompt = "Show setup's progress in the chat, and record it running.";
    pointing(prompt, &reply, vec![(RECORDING, MediaType::Webm, video::fixtures::RECORDING_WEBM.to_vec())])
}

/// The agent adding a setting, and pointing to a screenshot of it off and one of it on, each on a line of its own with
/// no empty line between them, which Markdown reads as one paragraph. The hub kept both.
pub(super) fn pointed_on_lines() -> World {
    let reply = format!(
        "Keeping the Mac awake is a setting of its own now, under the hub's settings. Off, then on:\n\n\
         ![The setting off]({OFF})\n![The setting on]({ON})\n\nBoth screenshots are in `docs/screens/`."
    );
    let prompt = "Add a setting for keeping the Mac awake, and show me how it looks off and on.";
    pointing(
        prompt,
        &reply,
        vec![(OFF, MediaType::Png, pictures::setting(false)), (ON, MediaType::Png, pictures::setting(true))],
    )
}

/// The agent adding the setting, and pointing to a screenshot of it among its words, which the hub kept.
pub(super) fn pointed_among_words() -> World {
    let reply = format!(
        "Keeping the Mac awake is a setting of its own now. Turned on, its row reads ![The setting on]({ON}) in the \
         hub's settings, and the screenshot is in `docs/screens/`."
    );
    let prompt = "Add a setting for keeping the Mac awake, and show me how its row looks.";
    pointing(prompt, &reply, vec![(ON, MediaType::Png, pictures::setting(true))])
}

/// The agent explaining how a prompt reaches it, with a Mermaid diagram, which the transcript draws.
pub(super) fn diagrammed() -> World {
    let reply = "Here's how a prompt reaches the agent now:\n\n\
                 ```mermaid\n\
                 flowchart LR\n  \
                   P[Prompt] --> Q{Agent working?}\n  \
                   Q -->|No| S[Sent at once]\n  \
                   Q -->|Yes| T[Steers its turn]\n  \
                   T -.->|Refused| W[Waits for the turn to end]\n\
                 ```\n\n\
                 The hub decides, so every client sends a prompt the same way.";
    let drawn = reply.find("```").map(|starts| format!("diagram/{starts}"));
    assert_eq!(drawn.as_deref(), Some(DIAGRAM_DRAWN), "the diagram is known by where it starts in the reply");
    pointing("How does a prompt reach the agent now? Draw it.", reply, Vec::new())
}

/// The chat of `prompt` and a reply to it that points to each of `kept`, an address in the worktree, and the kind and
/// the bytes of the copy the hub kept of the file there.
fn pointing(prompt: &str, reply: &str, kept: Vec<(&str, MediaType, Vec<u8>)>) -> World {
    let attachment = |(address, media_type, bytes): &(&str, MediaType, Vec<u8>)| Attachment {
        id: AttachmentId::from(format!("kept-{address}")),
        name: address.rsplit('/').next().unwrap_or(address).into(),
        media_type: *media_type,
        size_bytes: u64::try_from(bytes.len()).unwrap(),
    };
    let images: Vec<LinkedImage> =
        kept.iter().map(|kept| LinkedImage { address: kept.0.to_string(), attachment: attachment(kept) }).collect();
    let ids: Vec<AttachmentId> = images.iter().map(|image| image.attachment.id.clone()).collect();
    let events = first_turn(user_prompt(prompt), [text_pointing("msg-1", reply, images), replied(reply, 9, 0.05)]);
    let mut world = chat_world(SessionStatus::Idle, events);
    world.attachments.extend(ids.into_iter().zip(kept.into_iter().map(|(_, _, bytes)| bytes)));
    world
}
