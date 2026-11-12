//! Reading frames from a stream, as the hub reads them from its clients, its workers and providers' adapters.

use std::io;

use protocol::frame::{MAX_FRAME_BYTES, clear_buffer};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader};

/// Reads a frame at a time, refusing one larger than [`MAX_FRAME_BYTES`].
pub(crate) struct FrameReader<R> {
    reader: BufReader<R>,
    line: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ReadError {
    #[error("a frame was larger than {MAX_FRAME_BYTES} bytes")]
    Oversized,
    #[error("reading a frame")]
    Io(#[from] io::Error),
}

impl<R: AsyncRead + Unpin> FrameReader<R> {
    pub(crate) fn new(reader: R) -> Self {
        Self { reader: BufReader::new(reader), line: Vec::new() }
    }

    /// The next frame, as its line, or `None` once the stream has ended. A stream that ends partway through a line
    /// gives what it had of it. Nothing more can be read after an error, since the stream has lost its place.
    pub(crate) async fn next(&mut self) -> Result<Option<&[u8]>, ReadError> {
        clear_buffer(&mut self.line);
        let limit = u64::try_from(MAX_FRAME_BYTES).unwrap_or(u64::MAX).saturating_add(1);
        if (&mut self.reader).take(limit).read_until(b'\n', &mut self.line).await? == 0 {
            return Ok(None);
        }
        if self.line.len() > MAX_FRAME_BYTES {
            return Err(ReadError::Oversized);
        }
        Ok(Some(&self.line))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn frames_are_read_a_line_at_a_time_until_the_stream_ends() {
        let mut frames = FrameReader::new(&b"{\"a\":1}\n{\"b\":2}\n{\"c\""[..]);

        assert_eq!(frames.next().await.unwrap(), Some(&b"{\"a\":1}\n"[..]));
        assert_eq!(frames.next().await.unwrap(), Some(&b"{\"b\":2}\n"[..]));
        assert_eq!(frames.next().await.unwrap(), Some(&b"{\"c\""[..]), "what it had of the last");
        assert_eq!(frames.next().await.unwrap(), None);
    }

    #[tokio::test]
    async fn a_frame_too_large_is_refused_without_reading_all_of_it() {
        let (mut sending, receiving) = tokio::io::duplex(64 * 1024);
        // Never finished, so reading all of it would wait forever.
        tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            let chunk = vec![b'x'; 64 * 1024];
            while sending.write_all(&chunk).await.is_ok() {}
        });
        let mut frames = FrameReader::new(receiving);

        let read = frames.next().await;

        assert!(matches!(read, Err(ReadError::Oversized)), "{read:?}");
    }
}
