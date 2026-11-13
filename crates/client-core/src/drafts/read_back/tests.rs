// Against a stand-in hub on a Unix socket.
#![cfg(unix)]

use protocol::attachment::{Attachment, AttachmentBytes, MediaType};
use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, ReplyOutcome, attachments, drafts};

use super::*;
use crate::stand_in_hub::{self, Does};

fn kept(id: &str) -> Attachment {
    Attachment { id: id.into(), name: format!("{id}.txt"), media_type: MediaType::Text, size_bytes: 4 }
}

/// A hub that keeps a chat's file `chat` and the dialog's file `dialog`, each with its own bytes, and has no other.
fn hub_that_keeps(dir: &tempfile::TempDir) -> Client {
    stand_in_hub::connect(&dir.path().join("hub.sock"), |request| {
        // What each file it keeps is, by the request that reads it.
        let read = if let ClientRequest::AttachmentRead(attachments::AttachmentRead { id, .. }) = &request {
            (id.as_str() == "chat").then_some("the chat's")
        } else if let ClientRequest::NewWorkspaceDraftFileRead(drafts::NewWorkspaceDraftFileRead { id }) = &request {
            (id.as_str() == "dialog").then_some("the dialog's")
        } else {
            return Does::Nothing;
        };
        let Some(bytes) = read else {
            let missing = ApiError::new(ErrorCode::NotFound, "no such attachment");
            return Does::Answers(Box::new(ReplyOutcome::Error(missing)));
        };
        let data = AttachmentBytes::new(bytes.as_bytes().to_vec());
        let data = ClientResponse::AttachmentData(attachments::AttachmentData { data });
        Does::Answers(Box::new(ReplyOutcome::Response(data)))
    })
}

/// What came back, as each file's name and its bytes as text.
fn named(came_back: &CameBack) -> Vec<(&str, String)> {
    let files = came_back.files.iter();
    files.map(|file| (file.attachment.name.as_str(), String::from_utf8_lossy(&file.data).into_owned())).collect()
}

#[test]
fn a_chats_files_and_the_dialogs_are_each_read_from_where_the_hub_keeps_them() {
    let dir = tempfile::tempdir().unwrap();
    let client = hub_that_keeps(&dir);

    let chats = client.read_back(&FilesFor::Chat("s".into()), vec![kept("chat")]).came_back(&[kept("chat")]);
    let dialogs = client.read_back(&FilesFor::NewWorkspace, vec![kept("dialog")]).came_back(&[kept("dialog")]);

    assert_eq!(named(&chats.unwrap()), [("chat.txt", "the chat's".to_string())]);
    assert_eq!(named(&dialogs.unwrap()), [("dialog.txt", "the dialog's".to_string())]);
}

#[test]
fn one_the_hub_doesnt_have_is_said_by_name_and_the_rest_come_back_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let client = hub_that_keeps(&dir);
    let waiting = [kept("gone"), kept("chat")];

    let came_back = client.read_back(&FilesFor::Chat("s".into()), waiting.to_vec()).came_back(&waiting).unwrap();

    assert_eq!(named(&came_back), [("chat.txt", "the chat's".to_string())]);
    let unread: Vec<String> = came_back.unread.iter().map(ToString::to_string).collect();
    assert_eq!(unread.len(), 1);
    assert!(unread[0].starts_with("couldn't bring back gone.txt: "), "{unread:?}");
}

/// Such as one a change made elsewhere stopped naming, or one a newer read is on its way for.
#[test]
fn files_no_longer_waited_for_come_back_as_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let client = hub_that_keeps(&dir);
    let read = || client.read_back(&FilesFor::Chat("s".into()), vec![kept("chat")]);

    assert!(read().came_back(&[]).is_none(), "nothing's waited for");
    assert!(read().came_back(&[kept("chat"), kept("other")]).is_none(), "others are waited for too");
}

/// Read again on the next connection.
#[test]
fn a_read_the_connection_cut_off_comes_back_as_nothing() {
    let cut_off =
        ReadBack(vec![(kept("chat"), Ok(b"the chat's".to_vec())), (kept("more"), Err(ClientError::Disconnected))]);

    assert!(cut_off.came_back(&[kept("chat"), kept("more")]).is_none());
}
