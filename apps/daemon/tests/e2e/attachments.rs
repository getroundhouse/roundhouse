//! Files sent with a prompt, from the client to the agent, and back to anyone reading the chat, and the images in the
//! worktree that the agent's replies point to. What the hub keeps, refuses and gives the agent of a file is the hub's,
//! and its own tests in `hub/attachments/` prove it on stand-ins for its workers. These are what spans processes: the
//! files a real worker writes in a real worktree, and what it and the agent's adapter make of the images there.

#![cfg(test)]

use std::path::PathBuf;

use client_core::Client;
use protocol::attachment::{Attachment, AttachmentBytes, MediaType};
use protocol::event::{ContentBlock, EventBody, LinkedImage};
use protocol::model::{AttachmentId, Seq, SessionId};
use protocol::prompt::Prompt;

use crate::support::{Stack, assistant_text, git, wait_for_turn_end};

/// Enough of a PNG for anything that only looks at how it starts, which is all the hub and the fake agent do.
pub(crate) fn png(len: usize) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.resize(len, 7);
    bytes
}

fn add(client: &Client, session: &SessionId, id: &str, name: &str, media_type: MediaType, data: &[u8]) -> Attachment {
    client.attachment_add(session, &AttachmentId::from(id), name, media_type, data.to_vec()).unwrap()
}

#[test]
fn every_file_goes_in_the_worktree_and_the_agent_is_given_what_it_can_take_in_the_prompt() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    let worktree = PathBuf::from(workspace.path.unwrap());
    let shot = add(&client, &session.id, "a1", "/Users/someone/Desktop/shot.png", MediaType::Png, &png(300));
    let log = add(&client, &session.id, "a2", "build.log", MediaType::Text, b"error at line 42\nmore");
    let archive = add(&client, &session.id, "a3", "logs.zip", MediaType::Other, b"PK\x03\x04");
    let long_log = add(&client, &session.id, "a4", "long.log", MediaType::Text, &vec![b'a'; 40_000]);

    let carried = vec![shot.clone(), log.clone(), archive, long_log];
    let ids = carried.iter().map(|a| a.id.clone()).collect();
    client.session_prompt_with(&session.id, None, Prompt::carrying("attached", ids)).unwrap();

    let events = wait_for_turn_end(&client, &session.id, 1);
    assert_eq!(
        assistant_text(&events),
        "attached: image/png 300 bytes | shot.png: on disk, 300 bytes | build.log: error at line 42 \
         | logs.zip: on disk, 4 bytes | long.log: on disk, 40000 bytes"
    );
    let recorded = events.iter().find_map(|e| match &e.body {
        EventBody::UserPrompt { prompt: Prompt { files: attachments, .. }, .. } => Some(attachments.clone()),
        _ => None,
    });
    assert_eq!(recorded, Some(carried));
    assert_eq!(shot.name, "shot.png", "only the file's own name is kept");
    let folder = worktree.join(".context/attachments");
    assert_eq!(std::fs::read(folder.join("build.log")).unwrap(), b"error at line 42\nmore");
    assert_eq!(std::fs::read(folder.join("shot.png")).unwrap(), png(300));
    assert_eq!(git(&worktree, &["status", "--porcelain", "--untracked-files=all"]), "", "git never sees them");
}

/// The images the text of each of the agent's replies names, in order, by address.
fn linked(events: &[protocol::event::SessionEvent]) -> Vec<Vec<LinkedImage>> {
    let texts = events.iter().filter_map(|e| match &e.body {
        EventBody::Assistant { blocks, .. } => blocks.iter().find_map(|block| match block {
            ContentBlock::Text { images, .. } => Some(images.clone()),
            _ => None,
        }),
        _ => None,
    });
    texts.collect()
}

#[test]
fn an_image_a_reply_points_to_is_kept_as_it_was_when_the_reply_came() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    let worktree = PathBuf::from(workspace.path.clone().unwrap());
    std::fs::write(worktree.join("shot.png"), png(64)).unwrap();
    client.session_prompt(&session.id, "say:![The shot](shot.png)", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    std::fs::write(worktree.join("shot.png"), png(128)).unwrap();

    client.session_prompt(&session.id, "say:![The shot](shot.png)", None).unwrap();

    let events = wait_for_turn_end(&client, &session.id, 2);
    let [first, second] = linked(&events).try_into().unwrap_or_else(|all| panic!("{all:?}"));
    let ([first], [second]) = (first.as_slice(), second.as_slice()) else { panic!("{first:?} {second:?}") };
    assert_eq!((first.address.as_str(), second.address.as_str()), ("shot.png", "shot.png"));
    assert_eq!(client.attachment_read(&session.id, &first.attachment.id).unwrap(), png(64), "as it was then");
    assert_eq!(client.attachment_read(&session.id, &second.attachment.id).unwrap(), png(128));
}

#[test]
fn a_reply_names_no_image_that_isnt_one_inside_the_worktree() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    std::fs::write(PathBuf::from(workspace.path.unwrap()).join("notes.txt"), "not an image").unwrap();

    client.session_prompt(&session.id, "say:![a](notes.txt) ![b](../../x.png) ![c](missing.png)", None).unwrap();

    let events = wait_for_turn_end(&client, &session.id, 1);
    assert_eq!(linked(&events), [Vec::<LinkedImage>::new()]);
}

#[test]
fn an_image_a_tool_sends_back_is_kept_as_a_prompt_s_is_and_left_out_of_the_log() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    std::fs::write(PathBuf::from(workspace.path.unwrap()).join("shot.png"), png(300)).unwrap();

    client.session_prompt(&session.id, "read:shot.png", None).unwrap();

    wait_for_turn_end(&client, &session.id, 1);
    let (events, _) = client.session_read(&session.id, Seq::new(1), 100, true).unwrap();
    let result = events.iter().find_map(|e| match &e.body {
        EventBody::AgentUser { blocks, .. } => match blocks.as_slice() {
            [ContentBlock::ToolResult { images, .. }] => Some((images.clone(), e.raw.clone().unwrap())),
            _ => None,
        },
        _ => None,
    });
    let Some((images, raw)) = result else { panic!("{events:?}") };
    let [image] = images.as_slice() else { panic!("{images:?}") };
    assert_eq!((image.media_type, image.size_bytes), (MediaType::Png, 300));
    assert_eq!(client.attachment_read(&session.id, &image.id).unwrap(), png(300));
    let bytes = AttachmentBytes::new(png(300)).to_base64();
    assert!(
        raw.contains(image.id.as_str()) && !raw.contains(&bytes),
        "the line names it rather than holding it: {raw}"
    );
}
