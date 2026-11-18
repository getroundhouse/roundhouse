//! The requests that read what a session's log holds, without following it: its events by `seq`, or the agent's own or
//! one subagent's by place, its prompts and its compactions a line each, and which events hold some text. Each only
//! reads.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::event::SessionEvent;
use crate::find::Found;
use crate::model::{CompactionLine, Place, PromptLine, Seq, SessionId, SubagentId};

/// Reads events from `from_seq` on. Returns at least one event if any exist, and stops at `limit` events or about
/// `max_bytes`, as [`ReadLimit::asked`] says, so a huge event can't produce an unbounded reply.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionRead {
    pub id: SessionId,
    pub from_seq: Seq,
    pub limit: u32,
    pub max_bytes: Option<u64>,
    #[serde(default)]
    pub include_raw: bool,
}

impl Request for SessionRead {
    type Reply = Events;

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionRead(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Events {
    pub events: Vec<SessionEvent>,
    /// The session's newest event, so a reader knows how far the log goes.
    pub last_seq: Seq,
}

impl Reply for Events {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Events(events) = response else { return Err(response) };
        Ok(events)
    }
}

/// How much one read of a session's events sends, by `seq` or by place: at most `events` of them, and none after the
/// one that brings what they take to `bytes`, as their bodies, and the raw lines it sends, are written in the log. So it
/// sends at least one, however large.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadLimit {
    pub events: u32,
    pub bytes: u64,
}

impl ReadLimit {
    /// The most events a read sends, whatever it asks for.
    pub const MOST_EVENTS: u32 = 2000;
    /// The bytes a read that names none stops at.
    pub const BYTES: u64 = 4 * 1024 * 1024;

    /// What a read that asks for `limit` events and `max_bytes` is sent: at least one event, and at most
    /// [`ReadLimit::MOST_EVENTS`], up to [`ReadLimit::BYTES`] if it names none.
    pub fn asked(limit: u32, max_bytes: Option<u64>) -> Self {
        ReadLimit { events: limit.clamp(1, Self::MOST_EVENTS), bytes: max_bytes.unwrap_or(Self::BYTES) }
    }

    /// The events of `sized`, each with the bytes it takes, oldest first, that the read sends. It reads none after the
    /// last it sends, and one it can't read fails it.
    pub fn cut<T, E>(self, sized: impl IntoIterator<Item = Result<(T, u64), E>>) -> Result<Vec<T>, E> {
        let (mut sent, mut bytes) = (Vec::new(), 0);
        for event in sized.into_iter().take(usize::try_from(self.events).unwrap_or(usize::MAX)) {
            let (event, size) = event?;
            sent.push(event);
            bytes += size;
            if bytes >= self.bytes {
                break;
            }
        }
        Ok(sent)
    }
}

/// Reads the agent's own events, or those of the subagent `subagent`, from `from_place` on, as [`SessionRead`] reads
/// every event, up to the same [`ReadLimit`]. A hub older than this refuses it as unsupported, and a client reads by
/// `seq`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionReadPlaces {
    pub id: SessionId,
    pub subagent: Option<SubagentId>,
    pub from_place: Place,
    pub limit: u32,
    pub max_bytes: Option<u64>,
    #[serde(default)]
    pub include_raw: bool,
}

impl Request for SessionReadPlaces {
    type Reply = PlacedEvents;

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionReadPlaces(self)
    }
}

/// The agent's own events, or one subagent's.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlacedEvents {
    pub events: Vec<SessionEvent>,
    /// The place of the newest of them, so a reader knows how far they go. 0 when there are none.
    pub last_place: Place,
}

impl Reply for PlacedEvents {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::PlacedEvents(events) = response else { return Err(response) };
        Ok(events)
    }
}

/// Every prompt recorded in the session, oldest first, each as one line, for finding one in a long chat without
/// reading the whole log. A hub older than this refuses it as unsupported.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionPromptList {
    pub id: SessionId,
}

impl Request for SessionPromptList {
    type Reply = Prompts;

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionPromptList(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Prompts {
    pub prompts: Vec<PromptLine>,
}

impl Reply for Prompts {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Prompts(prompts) = response else { return Err(response) };
        Ok(prompts)
    }
}

/// Every compaction of the agent's own conversation that compacted, oldest first, each as one line, for marking them
/// in a long chat without reading the whole log. A hub older than this refuses it as unsupported.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionCompactionList {
    pub id: SessionId,
}

impl Request for SessionCompactionList {
    type Reply = Compactions;

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionCompactionList(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Compactions {
    pub compactions: Vec<CompactionLine>,
}

impl Reply for Compactions {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Compactions(compactions) = response else { return Err(response) };
        Ok(compactions)
    }
}

/// Which of the agent's own events, or those of the subagent `subagent`, hold `query`, by place, as
/// [`crate::find::Needle::is_in_event`] says, for finding text in a long chat without reading the whole log. An empty
/// query, or one longer than [`crate::find::Needle::MOST_CHARS`], is refused as invalid, and a hub older than this
/// refuses it as unsupported.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionFind {
    pub id: SessionId,
    pub subagent: Option<SubagentId>,
    pub query: String,
}

impl Request for SessionFind {
    type Reply = Found;

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionFind(self)
    }
}

impl Reply for Found {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Found(found) = response else { return Err(response) };
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use super::ReadLimit;

    /// What a read of events of these sizes is sent, by their index.
    fn sent(limit: ReadLimit, sizes: &[u64]) -> Vec<usize> {
        let Ok(sent) = limit.cut(sizes.iter().copied().enumerate().map(Ok::<_, Infallible>));
        sent
    }

    #[test]
    fn a_read_is_sent_at_least_one_event_and_at_most_as_many_as_the_hub_sends() {
        assert_eq!(ReadLimit::asked(0, None), ReadLimit { events: 1, bytes: ReadLimit::BYTES });
        assert_eq!(ReadLimit::asked(u32::MAX, Some(7)), ReadLimit { events: ReadLimit::MOST_EVENTS, bytes: 7 });
        assert_eq!(ReadLimit::asked(30, Some(0)).events, 30);
    }

    #[test]
    fn a_read_stops_after_the_event_that_brings_it_to_its_bytes_or_at_its_count() {
        let limit = |events, bytes| ReadLimit { events, bytes };

        assert_eq!(sent(limit(10, 10), &[4, 5, 1, 1]), [0, 1, 2], "the third brings it to 10");
        assert_eq!(sent(limit(10, 10), &[4, 5]), [0, 1], "it never got there");
        assert_eq!(sent(limit(10, 0), &[40, 1]), [0], "one, however large");
        assert_eq!(sent(limit(2, u64::MAX), &[1, 1, 1]), [0, 1]);
        assert_eq!(sent(limit(10, 10), &[]), [] as [usize; 0]);
    }

    #[test]
    fn a_read_fails_at_an_event_it_cant_read_and_reads_none_after_what_it_sends() {
        let mut read = 0;
        let sized = [Ok((0, 5)), Ok((1, 5)), Err("unread")].into_iter().inspect(|_| read += 1);

        assert_eq!(ReadLimit { events: 10, bytes: 10 }.cut(sized), Ok(vec![0, 1]));
        assert_eq!(read, 2, "the third was never read");
        let failing = [Ok((0, 1)), Err("broken")];
        assert_eq!(ReadLimit { events: 10, bytes: 10 }.cut(failing), Err("broken"));
    }
}
