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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{ContentBlock, EventBody};

    #[test]
    fn a_frame_is_exactly_one_line() {
        let body = EventBody::UserPrompt { text: "line one\nline two".into(), client_msg_id: None };
        let line = encode(&body).unwrap();
        assert_eq!(line.iter().position(|b| *b == b'\n'), Some(line.len() - 1));
        assert_eq!(line.last(), Some(&b'\n'));
        assert_eq!(decode::<EventBody>(&line).unwrap(), body);
    }

    #[test]
    fn unknown_variants_and_fields_decode_instead_of_failing() {
        let body: EventBody = decode(br#"{"type":"from_the_future","x":1}"#).unwrap();
        assert_eq!(body, EventBody::Unknown);
        let block: ContentBlock = decode(br#"{"type":"hologram"}"#).unwrap();
        assert_eq!(block, ContentBlock::Unknown);
        let body: EventBody =
            decode(br#"{"type":"user_prompt","text":"hi","client_msg_id":null,"later":true}"#).unwrap();
        assert_eq!(body, EventBody::UserPrompt { text: "hi".into(), client_msg_id: None });
    }
}
