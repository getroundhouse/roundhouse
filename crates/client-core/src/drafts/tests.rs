use std::path::Path;
use std::time::Duration;

use protocol::attachment::{Attachment, MediaType};
use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, ReplyOutcome, drafts};
use protocol::model::{AnswerDraft, CommentDraft, Workspace, WorkspaceDraft, WorkspaceId};

use super::*;
use crate::stand_in_hub::{self, Does};

mod answers_and_comments;
mod kinds;
mod new_workspace;

const TIMEOUT: Duration = Duration::from_secs(10);

/// How a fake hub answers each draft, and each file added or let go of.
#[derive(Clone, Copy)]
enum Answer {
    /// As a hub that has it does. It takes each file, and says the chat isn't there for each draft and each file let go
    /// of, which the keeper takes as an answer like any other, so the fake needn't make up a session.
    Answered,
    /// As a hub too old to keep drafts does.
    Refused,
    /// As a hub that keeps chats' drafts but not answers, comments or the new-workspace dialog does. It says `refused`
    /// before each it refuses.
    KeepsOnlyChatsDrafts,
    /// As `Answered`, but as a hub that knows no kind of file but one that goes as it is, `Other`, refuses the rest, as
    /// one older than the app refuses a kind newer than it. It says `refused` before each.
    KnowsNoKindOfFile,
    /// It goes away instead, as a hub that's stopped does.
    HangsUp,
}

/// What a fake hub was asked to keep, chat by chat, in the order it was asked: each draft's text, followed by the files
/// it names, and `+` or `-` and a file's ID for one added or let go of. An answer or a comment says which it is, then
/// what's typed in it, by its chat or its workspace. The new-workspace dialog's are the same as a chat's, by `dialog`.
type Heard = flume::Receiver<(String, String)>;

/// A hub that answers every draft and file as `answer` says, after saying what it was.
fn hub(socket: &Path, answer: Answer) -> (Client, Heard) {
    let (said, heard) = flume::unbounded();
    let client = stand_in_hub::connect(socket, move |request| {
        let added = added(&request);
        let not_a_chats = !matches!(
            request,
            ClientRequest::SessionDraft(_)
                | ClientRequest::AttachmentAdd { .. }
                | ClientRequest::AttachmentRemove { .. }
        );
        let Some((id, what)) = heard_of(request) else { return Does::Nothing };
        let code = match answer {
            Answer::KeepsOnlyChatsDrafts if not_a_chats => ErrorCode::Unsupported,
            Answer::KnowsNoKindOfFile if added.as_ref().is_some_and(|file| file.media_type != MediaType::Other) => {
                ErrorCode::Unsupported
            }
            Answer::Answered | Answer::KeepsOnlyChatsDrafts | Answer::KnowsNoKindOfFile => ErrorCode::NotFound,
            Answer::Refused => ErrorCode::Unsupported,
            Answer::HangsUp => return Does::HangsUp,
        };
        let refused = !matches!(answer, Answer::Refused) && code == ErrorCode::Unsupported;
        let _ = said.send((id, if refused { format!("refused {what}") } else { what }));
        let outcome = match added {
            Some(file) if code == ErrorCode::NotFound => ReplyOutcome::Response(ClientResponse::Attachment(file)),
            _ => ReplyOutcome::Error(ApiError::new(code, "a fake hub")),
        };
        Does::Answers(Box::new(outcome))
    });
    (client, heard)
}

/// The file a request adds, as a hub that takes it answers.
fn added(request: &ClientRequest) -> Option<Attachment> {
    let (ClientRequest::AttachmentAdd { id, name, media_type, data, .. }
    | ClientRequest::WorkspaceDraftFileAdd(drafts::WorkspaceDraftFileAdd { id, name, media_type, data })) = request
    else {
        return None;
    };
    let size_bytes = data.as_slice().len() as u64;
    Some(Attachment { id: id.clone(), name: name.clone(), media_type: *media_type, size_bytes })
}

/// What a fake hub says it heard of a request, for one the keeper makes.
fn heard_of(request: ClientRequest) -> Option<(String, String)> {
    if let ClientRequest::SessionDraft(drafts::SessionDraft { id, draft }) = request {
        let named = draft.files.iter().map(|file| format!(" {file}"));
        return Some((id.to_string(), named.fold(draft.text, |text, file| text + &file)));
    }
    if let ClientRequest::AttachmentAdd { session_id, id, .. } = request {
        return Some((session_id.to_string(), format!("+{id}")));
    }
    if let ClientRequest::AttachmentRemove { session_id, id } = request {
        return Some((session_id.to_string(), format!("-{id}")));
    }
    if let ClientRequest::SessionAnswerDraft(drafts::SessionAnswerDraft { id, draft }) = request {
        let typed = draft.and_then(|draft| draft.answers.first().map(|so_far| so_far.typed.clone()));
        return Some((id.to_string(), format!("answer {}", typed.as_deref().unwrap_or("none"))));
    }
    if let ClientRequest::WorkspaceCommentDraft(drafts::WorkspaceCommentDraft { id, draft }) = request {
        let text = draft.map(|draft| draft.text);
        return Some((id.to_string(), format!("comment {}", text.as_deref().unwrap_or("none"))));
    }
    let dialog = || "dialog".to_string();
    if let ClientRequest::WorkspaceDraftKeep(drafts::WorkspaceDraftKeep { draft }) = request {
        let named = draft.files.iter().map(|file| format!(" {file}"));
        return Some((dialog(), named.fold(draft.text, |text, file| text + &file)));
    }
    if let ClientRequest::WorkspaceDraftFileAdd(drafts::WorkspaceDraftFileAdd { id, .. }) = request {
        return Some((dialog(), format!("+{id}")));
    }
    if let ClientRequest::WorkspaceDraftFileRemove(drafts::WorkspaceDraftFileRemove { id }) = request {
        return Some((dialog(), format!("-{id}")));
    }
    None
}

fn chat(id: &str) -> SessionId {
    id.into()
}

fn file(id: &str) -> FileToSend {
    let mut file = FileToSend::checked("notes.txt", protocol::attachment::MediaType::Text, b"a line".to_vec()).unwrap();
    file.attachment.id = id.into();
    file
}

/// Waits until everything given so far is as kept as it can be, then says what the hub heard, in order.
fn kept(drafts: &Drafts, heard: &Heard) -> Vec<(String, String)> {
    drafts.flushed().recv_timeout(TIMEOUT).unwrap();
    heard.try_iter().collect()
}

fn pairs(said: &[(&str, &str)]) -> Vec<(String, String)> {
    said.iter().map(|(id, text)| ((*id).to_string(), (*text).to_string())).collect()
}

#[test]
fn only_the_newest_of_each_chat_is_kept_once_the_hub_has_answered_the_first() {
    let dir = tempfile::tempdir().unwrap();
    let (client, heard) = hub(&dir.path().join("hub.sock"), Answer::Answered);
    let drafts = Drafts::start(&client);
    drafts.typed(&chat("a"), NewDraft::new("h"));
    heard.recv_timeout(TIMEOUT).unwrap();

    for text in ["he", "hel", "hello"] {
        drafts.typed(&chat("a"), NewDraft::new(text));
    }
    drafts.typed(&chat("b"), NewDraft::new("other"));

    let mut later = kept(&drafts, &heard);
    later.sort();
    assert_eq!(later, pairs(&[("a", "hello"), ("b", "other")]), "a word typed isn't a call for each letter");
}

#[test]
fn typing_that_goes_on_is_still_kept_every_so_often() {
    let dir = tempfile::tempdir().unwrap();
    let (client, heard) = hub(&dir.path().join("hub.sock"), Answer::Answered);
    let drafts = Drafts::start(&client);
    drafts.typed(&chat("a"), NewDraft::new("h"));
    heard.recv_timeout(TIMEOUT).unwrap();
    let (stop, stopped) = flume::bounded::<()>(1);
    // A letter at a time, more often than the pause, until told to stop. Each change would put off a keeper that waits
    // for typing to stop, so it would never hear one.
    let typing = std::thread::spawn({
        let drafts = drafts.clone();
        move || {
            let mut text = String::new();
            while stopped.recv_timeout(PAUSE / 10).is_err() {
                text.push('a');
                drafts.typed(&chat("a"), NewDraft::new(text.clone()));
            }
        }
    });

    let during = heard.recv_timeout(TIMEOUT);

    stop.send(()).unwrap();
    typing.join().unwrap();
    assert!(during.unwrap().1.starts_with('a'), "something was kept while typing went on");
}

#[test]
fn a_change_kept_at_once_takes_the_place_of_one_still_waiting() {
    let dir = tempfile::tempdir().unwrap();
    let (client, heard) = hub(&dir.path().join("hub.sock"), Answer::Answered);
    let drafts = Drafts::start(&client);
    drafts.typed(&chat("a"), NewDraft::new("sent"));
    heard.recv_timeout(TIMEOUT).unwrap();

    drafts.typed(&chat("a"), NewDraft::new("sent, and more"));
    drafts.set(&chat("a"), NewDraft::default());

    assert_eq!(heard.recv_timeout(TIMEOUT).unwrap(), ("a".to_string(), String::new()));
    assert_eq!(kept(&drafts, &heard), [], "what it replaced is never kept, so it can't outlive it");
}

/// As when the app has updated but its hub hasn't yet, until the hub restarts as the build that keeps them.
#[test]
fn what_a_hub_that_doesnt_keep_drafts_refuses_waits_for_the_next_connection() {
    let dir = tempfile::tempdir().unwrap();
    let (old, heard_by_old) = hub(&dir.path().join("old.sock"), Answer::Refused);
    let drafts = Drafts::start(&old);
    drafts.typed(&chat("a"), NewDraft::new("h"));
    assert_eq!(kept(&drafts, &heard_by_old), pairs(&[("a", "h")]), "the first goes at once");
    let notes = file("f1");
    drafts.attach(&chat("b"), notes.clone());
    drafts.set(&chat("b"), NewDraft { text: "look".to_string(), files: vec![notes.attachment.id.clone()] });
    drafts.typed(&chat("a"), NewDraft::new("hi"));
    assert_eq!(kept(&drafts, &heard_by_old), [], "it's asked no more");
    assert!(drafts.unkept(&chat("a")) && drafts.unkept(&chat("b")));
    let (new, heard) = hub(&dir.path().join("new.sock"), Answer::Answered);

    drafts.reconnect(&new);

    let mut kept = kept(&drafts, &heard);
    kept.sort();
    assert_eq!(kept, pairs(&[("a", "hi"), ("b", "+f1"), ("b", "look f1")]));
    assert!(!drafts.unkept(&chat("a")) && !drafts.unkept(&chat("b")));
}

#[test]
fn what_hasnt_reached_the_hub_is_counted_until_it_has() {
    let dir = tempfile::tempdir().unwrap();
    let (gone, _) = hub(&dir.path().join("gone.sock"), Answer::HangsUp);
    let drafts = Drafts::start(&gone);

    drafts.typed(&chat("a"), NewDraft::new("typed as the hub went"));

    assert!(drafts.unkept(&chat("a")), "from the moment it's handed over");
    drafts.flushed().recv_timeout(TIMEOUT).unwrap();
    assert!(drafts.unkept(&chat("a")), "and while there's no hub to take it");
    assert!(!drafts.unkept(&chat("b")));
    let (client, heard) = hub(&dir.path().join("hub.sock"), Answer::Answered);
    drafts.reconnect(&client);
    kept(&drafts, &heard);
    assert!(!drafts.unkept(&chat("a")));
}

#[test]
fn a_draft_the_connection_couldnt_take_is_kept_on_the_next() {
    let dir = tempfile::tempdir().unwrap();
    let (gone, _) = hub(&dir.path().join("gone.sock"), Answer::HangsUp);
    let drafts = Drafts::start(&gone);
    drafts.typed(&chat("a"), NewDraft::new("typed while the hub went"));
    drafts.flushed().recv_timeout(TIMEOUT).unwrap();
    let (client, heard) = hub(&dir.path().join("hub.sock"), Answer::Answered);

    drafts.reconnect(&client);

    assert_eq!(kept(&drafts, &heard), pairs(&[("a", "typed while the hub went")]));
}

#[test]
fn flushed_answers_at_once_with_nothing_to_keep() {
    let dir = tempfile::tempdir().unwrap();
    let (client, _) = hub(&dir.path().join("hub.sock"), Answer::Answered);

    let drafts = Drafts::start(&client);

    drafts.flushed().recv_timeout(TIMEOUT).unwrap();
}

#[test]
fn a_file_reaches_the_hub_before_the_draft_that_names_it() {
    let dir = tempfile::tempdir().unwrap();
    let (gone, _) = hub(&dir.path().join("gone.sock"), Answer::HangsUp);
    let drafts = Drafts::start(&gone);
    drafts.typed(&chat("a"), NewDraft::new("first"));
    drafts.flushed().recv_timeout(TIMEOUT).unwrap();
    // Given while the hub was gone, so the next connection has both to keep at once.
    let notes = file("f1");
    drafts.set(&chat("a"), NewDraft { text: "look".to_string(), files: vec![notes.attachment.id.clone()] });
    drafts.attach(&chat("a"), notes);
    let (client, heard) = hub(&dir.path().join("hub.sock"), Answer::Answered);

    drafts.reconnect(&client);

    assert_eq!(kept(&drafts, &heard), pairs(&[("a", "+f1"), ("a", "look f1")]), "the hub leaves out one it hasn't got");
}

#[test]
fn a_file_let_go_of_waits_for_the_draft_that_stopped_naming_it() {
    let dir = tempfile::tempdir().unwrap();
    let (client, heard) = hub(&dir.path().join("hub.sock"), Answer::Answered);
    let drafts = Drafts::start(&client);
    drafts.typed(&chat("a"), NewDraft::new("h"));
    heard.recv_timeout(TIMEOUT).unwrap();
    drafts.typed(&chat("a"), NewDraft::new("without it"));

    drafts.detach(&chat("a"), &"f1".into());

    assert_eq!(kept(&drafts, &heard), pairs(&[("a", "without it"), ("a", "-f1")]), "the hub keeps any a draft names");
}

#[test]
fn a_file_this_connection_has_given_the_hub_isnt_added_again() {
    let dir = tempfile::tempdir().unwrap();
    let (client, heard) = hub(&dir.path().join("hub.sock"), Answer::Answered);
    let drafts = Drafts::start(&client);
    drafts.attach(&chat("a"), file("f1"));
    drafts.flushed().recv_timeout(TIMEOUT).unwrap();

    // As a composer does with the files of a prompt that comes back because it couldn't be sent.
    drafts.attach(&chat("a"), file("f1"));

    assert_eq!(kept(&drafts, &heard), pairs(&[("a", "+f1")]));
    assert!(!drafts.unkept(&chat("a")), "it counts as having reached the hub");
}

/// A hub that keeps every draft, answer and comment, and the new-workspace dialog's, and answers with the record it's
/// kept on, carrying it, at the next of its revisions, from `first`.
fn keeping_hub(socket: &Path, first: u64) -> Client {
    let mut next = first;
    stand_in_hub::connect(socket, move |request| {
        let kept = if let ClientRequest::SessionDraft(drafts::SessionDraft { id, draft }) = request {
            ClientResponse::Session(told(id.as_str(), next, &draft.text))
        } else if let ClientRequest::SessionAnswerDraft(drafts::SessionAnswerDraft { id, draft }) = request {
            ClientResponse::Session(Session { answer_draft: draft, ..told(id.as_str(), next, "") })
        } else if let ClientRequest::WorkspaceCommentDraft(drafts::WorkspaceCommentDraft { id, draft }) = request {
            ClientResponse::Workspace(writing(id.as_str(), next, draft))
        } else if let ClientRequest::WorkspaceDraftKeep(drafts::WorkspaceDraftKeep { draft }) = request {
            ClientResponse::WorkspaceDraft(WorkspaceDraft {
                rev: Rev::new(next),
                text: draft.text,
                ..Default::default()
            })
        } else {
            return Does::Nothing;
        };
        next += 1;
        Does::Answers(Box::new(ReplyOutcome::Response(kept)))
    })
}

/// The chat's record as the hub's feed might tell of it, at `rev`, with `typed` in its composer.
fn told(id: &str, rev: u64, typed: &str) -> Session {
    let draft = Draft { text: typed.to_string(), files: Vec::new() };
    Session { rev: Rev::new(rev), draft, ..protocol::fixtures::session(id) }
}

/// The workspace's record as the hub's feed might tell of it, at `rev`, with `comment` being written on its diff.
fn writing(id: &str, rev: u64, comment: Option<CommentDraft>) -> Workspace {
    Workspace { rev: Rev::new(rev), comment_draft: comment, ..protocol::fixtures::workspace(id) }
}

#[test]
fn a_newer_record_is_from_elsewhere_only_when_its_draft_isnt_the_one_kept_from_here() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::start(&keeping_hub(&dir.path().join("hub.sock"), 5));

    drafts.set(&chat("a"), NewDraft::new("typed here"));
    drafts.flushed().recv_timeout(TIMEOUT).unwrap();

    assert!(!drafts.from_elsewhere(&told("a", 5, "typed here")), "the record the hub kept it in is this client's");
    assert!(!drafts.from_elsewhere(&told("a", 4, "typed before")), "and so is an older one that arrives late");
    assert!(
        !drafts.from_elsewhere(&told("a", 6, "typed here")),
        "and one newer for something else, such as its status"
    );
    assert!(drafts.from_elsewhere(&told("a", 7, "typed elsewhere")));
    assert!(drafts.from_elsewhere(&told("b", 1, "typed elsewhere")), "nothing was typed there here");
    assert!(!drafts.from_elsewhere(&told("c", 1, "")), "nor anything elsewhere");
}

#[test]
fn a_draft_taken_from_a_record_isnt_from_elsewhere_until_another_is_made_there() {
    let dir = tempfile::tempdir().unwrap();
    let (client, _) = hub(&dir.path().join("hub.sock"), Answer::Answered);
    let drafts = Drafts::start(&client);
    let elsewhere = told("a", 3, "typed elsewhere");
    assert!(drafts.from_elsewhere(&elsewhere));

    drafts.took(&elsewhere);

    assert!(!drafts.from_elsewhere(&elsewhere));
    assert!(!drafts.from_elsewhere(&told("a", 4, "typed elsewhere")), "a newer record that carries it");
    assert!(drafts.from_elsewhere(&told("a", 5, "typed elsewhere again")));
}

#[test]
fn nothing_from_elsewhere_is_taken_while_a_draft_from_here_is_on_its_way() {
    let dir = tempfile::tempdir().unwrap();
    let (gone, _) = hub(&dir.path().join("gone.sock"), Answer::HangsUp);
    let drafts = Drafts::start(&gone);
    drafts.typed(&chat("a"), NewDraft::new("typed as the hub went"));
    drafts.flushed().recv_timeout(TIMEOUT).unwrap();

    assert!(!drafts.from_elsewhere(&told("a", 100, "typed elsewhere")), "what's typed here goes on and replaces it");
    assert!(drafts.caught_up().try_recv().is_err());
    drafts.reconnect(&keeping_hub(&dir.path().join("hub.sock"), 5));

    drafts.caught_up().recv_timeout(TIMEOUT).unwrap();
    assert!(!drafts.unkept(&chat("a")));
    assert!(drafts.from_elsewhere(&told("a", 100, "typed elsewhere")), "a newer record, once nothing is on its way");
    assert!(!drafts.from_elsewhere(&told("a", 5, "typed as the hub went")));
}
