//! One JSON value per line. The callers own the IO.

use serde::Serialize;
use serde::de::DeserializeOwned;

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
        let body = EventBody::UserPrompt {
            text: "line one\nline two".into(),
            client_msg_id: None,
            attachments: Vec::new(),
            tracked: false,
        };
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
        assert_eq!(
            body,
            EventBody::UserPrompt { text: "hi".into(), client_msg_id: None, attachments: Vec::new(), tracked: false }
        );
        assert_eq!(decode::<WorkspaceStatus>(br#""from_the_future""#).unwrap(), WorkspaceStatus::Unknown);
        assert_eq!(decode::<SessionStatus>(br#""from_the_future""#).unwrap(), SessionStatus::Unknown);
        assert_eq!(decode::<AgentKind>(br#""from_the_future""#).unwrap(), AgentKind::Unknown);
        assert_eq!(decode::<DiffSide>(br#""from_the_future""#).unwrap(), DiffSide::Unknown);
        assert_eq!(decode::<FileChange>(br#"{"kind":"renamed","to":"b.rs"}"#).unwrap(), FileChange::Unknown);
        assert_eq!(decode::<LineMark>(br#""moved""#).unwrap(), LineMark::Unknown);
    }
}
