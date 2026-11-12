//! The calls in flight on a stream that carries calls one way and their replies the other, matched by ID, as a
//! worker's stream and a provider's adapter's both do, and any other line sent the calls' way, such as the answer to
//! something the other end asked.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::Mutex;
use protocol::frame;
use serde::Serialize;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};
use tracing::warn;

/// Why a call, or another line, wasn't sent.
#[derive(Debug, thiserror::Error)]
pub(crate) enum NotSent {
    #[error("the stream has closed")]
    Closed,
    #[error("the line didn't encode")]
    Encoding(#[from] serde_json::Error),
}

pub(crate) struct Calls<O> {
    next_id: AtomicU64,
    /// Lines for the task that writes them, so a call dropped partway can't leave half of one. Dropping this closes
    /// the stream's write half once they're written, which is how the other end is told to stop.
    lines: Mutex<Option<mpsc::Sender<Vec<u8>>>>,
    /// `None` once the stream has ended, so a call can't wait for a reply that won't come.
    waiting: Mutex<Option<HashMap<u64, oneshot::Sender<O>>>>,
}

impl<O> Calls<O> {
    pub(crate) fn new(writer: Box<dyn AsyncWrite + Send + Unpin>) -> Self {
        let (lines, queued) = mpsc::channel(256);
        tokio::spawn(write_lines(writer, queued));
        Self { next_id: AtomicU64::new(1), lines: Mutex::new(Some(lines)), waiting: Mutex::new(Some(HashMap::new())) }
    }

    /// Queues the call `call` makes of its ID, and returns its reply to wait for, which is dropped unsent if the
    /// stream ends first. Calls are written in the order they're queued.
    pub(crate) async fn send<C: Serialize>(
        &self,
        call: impl FnOnce(u64) -> C,
    ) -> Result<oneshot::Receiver<O>, NotSent> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let line = frame::encode(&call(id))?;
        let lines = self.lines.lock().clone().ok_or(NotSent::Closed)?;
        let (reply, replied) = oneshot::channel();
        self.waiting.lock().as_mut().ok_or(NotSent::Closed)?.insert(id, reply);
        // Not held while the reply is waited for. A long call would otherwise keep the stream open after `close`.
        if lines.send(line).await.is_err() {
            if let Some(waiting) = self.waiting.lock().as_mut() {
                waiting.remove(&id);
            }
            return Err(NotSent::Closed);
        }
        Ok(replied)
    }

    /// Queues a line that isn't a call, such as the answer to something the other end asked, after what's queued.
    pub(crate) async fn write(&self, line: &impl Serialize) -> Result<(), NotSent> {
        let line = frame::encode(line)?;
        let lines = self.lines.lock().clone().ok_or(NotSent::Closed)?;
        lines.send(line).await.map_err(|_| NotSent::Closed)
    }

    /// Gives the reply to the call it's for, if that's still waiting.
    pub(crate) fn reply(&self, id: u64, outcome: O) {
        let reply = self.waiting.lock().as_mut().and_then(|waiting| waiting.remove(&id));
        if let Some(reply) = reply {
            let _ = reply.send(outcome);
        }
    }

    /// Closes the stream's write half, once what's queued is written, and sends nothing more.
    pub(crate) fn close(&self) {
        self.lines.lock().take();
    }

    /// For when the stream has ended. Every call still waiting is woken without its reply.
    pub(crate) fn end(&self) {
        self.close();
        drop(self.waiting.lock().take());
    }

    /// How many calls wait for their reply.
    pub(crate) fn waiting(&self) -> usize {
        self.waiting.lock().as_ref().map_or(0, HashMap::len)
    }

    /// Whether calls can still be sent.
    pub(crate) fn is_open(&self) -> bool {
        self.lines.lock().is_some() && self.waiting.lock().is_some()
    }
}

async fn write_lines(mut writer: Box<dyn AsyncWrite + Send + Unpin>, mut lines: mpsc::Receiver<Vec<u8>>) {
    while let Some(line) = lines.recv().await {
        if let Err(e) = writer.write_all(&line).await.and(writer.flush().await) {
            warn!("writing a call failed: {e}");
            break;
        }
    }
    let _ = writer.shutdown().await;
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::frames::FrameReader;

    #[tokio::test]
    async fn replies_reach_their_calls_whatever_order_they_come_in() {
        let (ours, theirs) = tokio::io::duplex(1024);
        let calls = Calls::<&str>::new(Box::new(ours));
        let first = calls.send(|id| json!({"id": id, "ask": "first"})).await.unwrap();
        let second = calls.send(|id| json!({"id": id, "ask": "second"})).await.unwrap();
        let mut sent = FrameReader::new(theirs);
        let mut ids = Vec::new();
        for ask in ["first", "second"] {
            let call: Value = frame::decode(sent.next().await.unwrap().unwrap()).unwrap();
            assert_eq!(call["ask"], ask, "written in the order they were queued");
            ids.push(call["id"].as_u64().unwrap());
        }

        calls.reply(ids[1], "to the second");
        calls.reply(ids[0], "to the first");

        assert_eq!((first.await.unwrap(), second.await.unwrap()), ("to the first", "to the second"));
    }

    #[tokio::test]
    async fn a_call_waiting_when_the_stream_ends_is_woken_and_none_is_sent_after() {
        let (ours, _theirs) = tokio::io::duplex(1024);
        let calls = Calls::<()>::new(Box::new(ours));
        let waiting = calls.send(|id| json!({"id": id})).await.unwrap();

        calls.end();

        assert!(waiting.await.is_err());
        assert!(matches!(calls.send(|id| json!({"id": id})).await, Err(NotSent::Closed)));
        assert!(!calls.is_open());
    }

    #[tokio::test]
    async fn a_line_that_isnt_a_call_goes_after_whats_queued_and_none_goes_once_its_closed() {
        let (ours, theirs) = tokio::io::duplex(1024);
        let calls = Calls::<()>::new(Box::new(ours));
        let _waiting = calls.send(|id| json!({"id": id})).await.unwrap();

        calls.write(&json!({"answer": 9})).await.unwrap();
        calls.close();

        let mut sent = FrameReader::new(theirs);
        let call: Value = frame::decode(sent.next().await.unwrap().unwrap()).unwrap();
        let answer: Value = frame::decode(sent.next().await.unwrap().unwrap()).unwrap();
        assert!(call["id"].is_u64(), "{call}");
        assert_eq!(answer, json!({"answer": 9}));
        assert!(matches!(calls.write(&json!({"answer": 10})).await, Err(NotSent::Closed)));
    }

    #[tokio::test]
    async fn closing_ends_the_stream_once_whats_queued_is_written() {
        let (ours, theirs) = tokio::io::duplex(1024);
        let calls = Calls::<()>::new(Box::new(ours));
        let _waiting = calls.send(|id| json!({"id": id})).await.unwrap();

        calls.close();

        let mut sent = FrameReader::new(theirs);
        assert!(sent.next().await.unwrap().is_some(), "what was queued is still written");
        assert!(sent.next().await.unwrap().is_none(), "then the stream ends");
    }
}
