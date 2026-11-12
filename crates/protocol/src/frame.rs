//! One JSON value per line. The callers own the IO.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Far above the largest event seen in real sessions (2.3 MB), but stops a corrupt stream from
/// growing a buffer without limit.
pub const MAX_FRAME_BYTES: usize = 256 * 1024 * 1024;

pub fn encode<T: Serialize>(value: &T) -> serde_json::Result<Vec<u8>> {
    let mut line = serde_json::to_vec(value)?;
    line.push(b'\n');
    Ok(line)
}

pub fn decode<T: DeserializeOwned>(line: &[u8]) -> serde_json::Result<T> {
    serde_json::from_slice(line)
}

/// A request as its reader reads it: the call it's for, and the request, unless this build can't read it.
#[derive(Debug)]
pub struct Call<R> {
    pub id: u64,
    pub request: Result<R, serde_json::Error>,
}

/// Reads a request frame, `{"id":…,"request":…}`, as a client, a hub and a provider adapter each send one. A request
/// this build can't read as a whole, such as one with a value it doesn't know, is from a newer peer, so it still comes
/// back with its call's `id`, for the reader to refuse once it has checked what it checks of any request. Leaving it
/// unanswered would hold the call until the connection closes. Fails only for a frame that names no call.
pub fn read_call<R: DeserializeOwned>(line: &[u8]) -> serde_json::Result<Call<R>> {
    #[derive(Deserialize)]
    #[serde(bound = "R: DeserializeOwned")]
    struct Whole<R> {
        id: u64,
        request: R,
    }
    #[derive(Deserialize)]
    struct Named {
        id: u64,
    }
    match decode::<Whole<R>>(line) {
        Ok(Whole { id, request }) => Ok(Call { id, request: Ok(request) }),
        Err(error) => Ok(Call { id: decode::<Named>(line)?.id, request: Err(error) }),
    }
}

/// The call a reply was for, when the reply as a whole didn't decode, so the call can still be ended. The frames that
/// carry replies, from a hub, a worker or a provider's adapter, all say `"frame":"reply"`, and only a reply names an
/// `id` there.
pub fn reply_id(line: &[u8]) -> Option<u64> {
    #[derive(Deserialize)]
    struct Reply {
        frame: String,
        id: Option<u64>,
    }
    let reply = decode::<Reply>(line).ok()?;
    if reply.frame == "reply" { reply.id } else { None }
}

/// The most a buffer for reading frames keeps between them.
const KEPT_BUFFER_BYTES: usize = 1024 * 1024;

/// Empties a buffer that frames are read into, for the next. One that grew to take a large frame, such as a file a
/// prompt carries, is let go, or the connection would hold that much for as long as it lasts.
pub fn clear_buffer(buffer: &mut Vec<u8>) {
    if buffer.capacity() > KEPT_BUFFER_BYTES {
        *buffer = Vec::new();
    } else {
        buffer.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{ContentBlock, EventBody, FileChange, LineMark};
    use crate::model::{AgentKind, DiffSide, RepoId, SessionId, SessionStatus, WorkspaceId, WorkspaceStatus};

    #[test]
    fn a_buffer_that_took_a_large_frame_is_let_go() {
        let (mut small, mut large) = (Vec::with_capacity(64), vec![0; KEPT_BUFFER_BYTES + 1]);
        small.push(1);

        clear_buffer(&mut small);
        clear_buffer(&mut large);

        assert!(small.is_empty() && small.capacity() >= 64, "a small one is kept to use again");
        assert_eq!(large.capacity(), 0);
    }

    #[test]
    fn a_frame_is_exactly_one_line() {
        let body =
            EventBody::UserPrompt { text: "line one\nline two".into(), client_msg_id: None, attachments: Vec::new() };
        let line = encode(&body).unwrap();
        assert_eq!(line.iter().position(|b| *b == b'\n'), Some(line.len() - 1));
        assert_eq!(line.last(), Some(&b'\n'));
        assert_eq!(decode::<EventBody>(&line).unwrap(), body);
    }

    #[test]
    fn ids_are_plain_strings_on_the_wire() {
        assert_eq!(encode(&SessionId::from("abc")).unwrap(), b"\"abc\"\n");
        assert_eq!(decode::<SessionId>(br#""abc""#).unwrap(), SessionId::from("abc"));
        assert_eq!(decode::<WorkspaceId>(br#""abc""#).unwrap(), WorkspaceId::from("abc"));
        assert_eq!(decode::<RepoId>(br#""abc""#).unwrap(), RepoId::from("abc"));
    }

    #[test]
    fn unknown_variants_and_fields_decode_instead_of_failing() {
        let body: EventBody = decode(br#"{"type":"from_the_future","x":1}"#).unwrap();
        assert_eq!(body, EventBody::Unknown);
        let block: ContentBlock = decode(br#"{"type":"hologram"}"#).unwrap();
        assert_eq!(block, ContentBlock::Unknown);
        let body: EventBody =
            decode(br#"{"type":"user_prompt","text":"hi","client_msg_id":null,"later":true}"#).unwrap();
        assert_eq!(body, EventBody::UserPrompt { text: "hi".into(), client_msg_id: None, attachments: Vec::new() });
        assert_eq!(decode::<WorkspaceStatus>(br#""from_the_future""#).unwrap(), WorkspaceStatus::Unknown);
        assert_eq!(decode::<SessionStatus>(br#""from_the_future""#).unwrap(), SessionStatus::Unknown);
        assert_eq!(decode::<AgentKind>(br#""from_the_future""#).unwrap(), AgentKind::Unknown);
        assert_eq!(decode::<DiffSide>(br#""from_the_future""#).unwrap(), DiffSide::Unknown);
        assert_eq!(decode::<FileChange>(br#"{"kind":"renamed","to":"b.rs"}"#).unwrap(), FileChange::Unknown);
        assert_eq!(decode::<LineMark>(br#""moved""#).unwrap(), LineMark::Unknown);
    }
}
