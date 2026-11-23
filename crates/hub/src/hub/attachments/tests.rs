//! What the hub keeps of a file added to a chat, what it refuses, and what a prompt that carries one gives the agent,
//! on a hub whose workspaces have stand-ins for their workers, which put each file a prompt carries in the worktree.

use protocol::attachment::{Attachment, AttachmentBytes, AttachmentFile, MediaType, NewAttachment};
use protocol::capability::Control;
use protocol::capability::settings::Settings;
use protocol::client::{ApiError, ErrorCode};
use protocol::event::EventBody;
use protocol::model::{AgentKind, AttachmentId, ByteRange, Seq, SessionId};
use protocol::prompt::Prompt;
use protocol::worker::WorkerRequest;

use crate::attachment_files::FileFor;
use crate::hub::Hub;
use crate::hub::stand_in_workers::{Asked, a_chat, a_turn, hub_with_stand_in_workers};
use crate::hub::tests::hub_in;
use crate::store::AddedBy;

/// Enough of a PNG for anything that only looks at how it starts, which is all the hub does.
fn png(len: usize) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.resize(len, 7);
    bytes
}

/// Adds `data` to the chat for a prompt to carry, as a client does.
async fn add(
    hub: &Hub,
    chat: &SessionId,
    id: &str,
    name: &str,
    media_type: MediaType,
    data: &[u8],
) -> Result<Attachment, ApiError> {
    let (kept_for, data) = (FileFor::Session(chat.clone()), AttachmentBytes::new(data.to_vec()));
    hub.file_add(kept_for, id.into(), name.into(), media_type, data, AddedBy::Client).await
}

/// The bytes of the file the chat has under `id`, as a client reads them, or the code it's refused with.
async fn read(hub: &Hub, chat: &SessionId, id: &AttachmentId) -> Result<Vec<u8>, ErrorCode> {
    let read = hub.file_read(FileFor::Session(chat.clone()), id.clone()).await;
    read.map(AttachmentBytes::into_vec).map_err(|e| e.code)
}

/// What the agent was last given of a prompt.
fn given_last(asked: &Asked, chat: &SessionId) -> Prompt<AttachmentFile> {
    let given = asked.requests(|request| {
        let WorkerRequest::AgentControl(given) = request else { return None };
        let Control::Prompt { prompt, .. } = &given.control else { return None };
        (given.session_id == *chat).then(|| prompt.clone())
    });
    given.last().expect("the agent was given a prompt").clone()
}

fn log(hub: &Hub, chat: &SessionId) -> Vec<protocol::event::SessionEvent> {
    hub.store.read_events(chat, Seq::FIRST, 1000, u64::MAX, false).unwrap()
}

fn s() -> SessionId {
    "s".into()
}

#[tokio::test]
async fn a_chat_refuses_a_file_that_isnt_what_it_says_and_records_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());

    let renamed = add(&hub, &s(), "a1", "shot.png", MediaType::Png, &[0xff, 0xd8, 0xff, 0xe0]).await;
    let not_text = add(&hub, &s(), "a1", "a.bin", MediaType::Text, &[0, 1, 2]).await;
    let empty = add(&hub, &s(), "a1", "empty.log", MediaType::Text, &[]).await;
    let unknown = add(&hub, &s(), "a1", "a.pdf", MediaType::Unknown, b"%PDF-1.4").await;
    let climbing = add(&hub, &s(), "../../hub", "a.log", MediaType::Text, b"x").await;
    let shouting = add(&hub, &s(), "A1", "a.log", MediaType::Text, b"x").await;

    for refused in [renamed, not_text, empty, climbing, shouting] {
        assert_eq!(refused.map_err(|e| e.code).unwrap_err(), ErrorCode::InvalidRequest);
    }
    let unknown = unknown.map_err(|e| e.code).unwrap_err();
    assert_eq!(unknown, ErrorCode::Unsupported, "so a newer client can send it as a plain file");
    assert_eq!(read(&hub, &s(), &"a1".into()).await, Err(ErrorCode::NotFound));
    assert!(!dir.path().join("attachments").join("s").exists(), "nothing was written");
}

#[tokio::test]
async fn adding_the_same_attachment_again_keeps_the_first() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let first = add(&hub, &s(), "a1", "one.log", MediaType::Text, b"one").await.unwrap();

    let again = add(&hub, &s(), "a1", "two.log", MediaType::Text, b"two").await.unwrap();

    assert_eq!(again, first);
    assert_eq!(read(&hub, &s(), &first.id).await.unwrap(), b"one");
}

/// Files are added just before the prompt that carries them, so these are from a prompt that never arrived.
#[tokio::test]
async fn closing_a_chat_whose_files_no_prompt_carried_deletes_them_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let log = add(&hub, &s(), "a1", "build.log", MediaType::Text, b"error at line 42").await.unwrap();
    let kept = dir.path().join("attachments").join("s");
    assert!(kept.join(log.id.as_str()).exists());

    hub.session_close(s()).await.unwrap();

    assert!(!kept.exists());
    assert_eq!(read(&hub, &s(), &log.id).await, Err(ErrorCode::NotFound));
}

/// Once a prompt has had the worker put its files in the worktree, the hub keeps the bytes only of an image, since
/// that's all the transcript draws, and a later prompt that carries another of them carries it from where it went.
#[tokio::test]
async fn the_hub_keeps_an_image_once_its_sent_and_lets_go_of_anything_else() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (workspace, chat) = a_chat(&hub).await;
    let shot = add(&hub, &chat.id, "a1", "shot.png", MediaType::Png, &png(64)).await.unwrap();
    let log = add(&hub, &chat.id, "a2", "build.log", MediaType::Text, b"error at line 42").await.unwrap();
    let until_its_sent = read(&hub, &chat.id, &log.id).await;

    let both = Prompt::carrying("attached", vec![shot.id.clone(), log.id.clone()]);
    a_turn(&hub, &asked, &chat.id, both, None).await;
    let first = given_last(&asked, &chat.id);
    let (shot_after, log_after) = (read(&hub, &chat.id, &shot.id).await, read(&hub, &chat.id, &log.id).await);
    a_turn(&hub, &asked, &chat.id, Prompt::carrying("attached", vec![log.id.clone()]), None).await;
    let again = given_last(&asked, &chat.id);

    assert_eq!(until_its_sent.unwrap(), b"error at line 42");
    assert_eq!(shot_after.unwrap(), png(64));
    assert_eq!(log_after, Err(ErrorCode::NotFound));
    let worktree = workspace.path.unwrap();
    let placed = |prompt: &Prompt<AttachmentFile>| -> Vec<(AttachmentId, String)> {
        prompt.files.iter().map(|file| (file.attachment.id.clone(), file.path.clone())).collect()
    };
    let in_the_worktree = |name: &str| format!("{worktree}/.context/attachments/{name}");
    let log_placed = (log.id.clone(), in_the_worktree("build.log"));
    assert_eq!(placed(&first), [(shot.id.clone(), in_the_worktree("shot.png")), log_placed.clone()]);
    assert_eq!(placed(&again), [log_placed], "from where it went");
    let written = |request: &WorkerRequest| matches!(request, WorkerRequest::AttachmentWrite(_)).then_some(());
    assert_eq!(asked.requests(written).len(), 2, "each was put in the worktree once");
}

/// A prompt can be only files, but with none it needs text. It carries only files added to its own chat, and no more
/// than ten, and nothing is recorded of one that's refused.
#[tokio::test]
async fn a_prompt_can_be_only_files_and_can_only_carry_what_was_added_to_its_own_chat() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (workspace, chat) = a_chat(&hub).await;
    let unchanged = Settings::default();
    let other = hub.session_create(workspace.id.clone(), AgentKind::Claude, unchanged, None).await.unwrap().id;
    let shot = add(&hub, &other, "a1", "shot.png", MediaType::Png, &png(64)).await.unwrap();
    let prompt = |chat: &SessionId, prompt: Prompt<AttachmentId>| hub.session_prompt(chat.clone(), None, prompt, None);

    let elsewhere = prompt(&chat.id, Prompt::carrying("attached", vec![shot.id.clone()])).await;
    let too_many = prompt(&other, Prompt::carrying("attached", vec![shot.id.clone(); 11])).await;
    let says_nothing = prompt(&other, Prompt::new(" ")).await;
    a_turn(&hub, &asked, &other, Prompt::carrying("", vec![shot.id.clone()]), None).await;

    assert_eq!(elsewhere.map_err(|e| e.code).unwrap_err(), ErrorCode::NotFound);
    assert_eq!(too_many.map_err(|e| e.code).unwrap_err(), ErrorCode::InvalidRequest);
    assert_eq!(
        says_nothing.map_err(|e| e.code).unwrap_err(),
        ErrorCode::InvalidRequest,
        "with no files, it needs text"
    );
    assert_eq!(read(&hub, &chat.id, &shot.id).await, Err(ErrorCode::NotFound));
    assert!(log(&hub, &chat.id).is_empty(), "nothing is recorded for a refused prompt");
    let recorded = log(&hub, &other).into_iter().filter_map(|e| {
        let EventBody::UserPrompt { prompt, .. } = e.body else { return None };
        Some(prompt.files)
    });
    assert_eq!(recorded.collect::<Vec<_>>(), [vec![shot]], "only the prompt of only files");
}

/// What the agent's text points to is kept if the transcript shows it, an image whole, and a video it plays a part at a
/// time, and anything else a worker sends is left out. A video a person added is read whole, as a draft's files are.
#[tokio::test]
async fn the_hub_keeps_what_the_transcript_shows_of_the_agent_s() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let (webm, mp4) = (protocol::fixtures::webm(1000), b"\0\0\0\x18ftypisom\0\0\x02\0".to_vec());
    let sent = |id: &str, media_type, data: &[u8]| NewAttachment {
        attachment: Attachment {
            id: id.into(),
            name: id.into(),
            media_type,
            size_bytes: data.len().try_into().unwrap(),
        },
        data: AttachmentBytes::new(data.to_vec()),
    };

    let kept =
        [sent("a1", MediaType::Png, &png(64)), sent("a2", MediaType::Webm, &webm), sent("a3", MediaType::Mp4, &mp4)];
    hub.keep_from_agent(&s(), kept.to_vec()).await;
    add(&hub, &s(), "a4", "mine.webm", MediaType::Webm, &webm).await.unwrap();

    let part = hub.file_read_range(FileFor::Session(s()), "a2".into(), ByteRange::new(4, 8).unwrap()).await.unwrap();
    assert_eq!(read(&hub, &s(), &"a1".into()).await.unwrap(), png(64));
    assert_eq!((part.data.as_slice(), part.file_bytes), (&webm[4..12], 1000));
    assert_eq!(read(&hub, &s(), &"a2".into()).await, Err(ErrorCode::InvalidRequest), "not whole");
    assert_eq!(read(&hub, &s(), &"a4".into()).await.unwrap(), webm, "a person's");
    assert_eq!(read(&hub, &s(), &"a3".into()).await, Err(ErrorCode::NotFound), "a video the app doesn't play");
}
