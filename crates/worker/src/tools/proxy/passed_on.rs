use super::Asked;
use super::message::Message;
use super::under_way::Counted;
use hyper::Method;
use hyper::body::{Body, Bytes, Frame, Incoming, SizeHint};
use std::pin::Pin;
use std::task::{Context, Poll};

/// What a server on protocol `2026-07-28` is asked to send its events with, where one on those before is sent a GET.
const LISTEN: &str = "subscriptions/listen";

pub(super) type Gone = Pin<Box<dyn Future<Output = ()> + Send>>;

/// The body of a server's answer, as it comes. A server's stream of events, which a GET asks for, or a POST of
/// [`LISTEN`], stays open for as long as an agent is connected, so it ends as the server is moved to another URL, or
/// removed. A call's answer goes on as it began, and the call is counted as under way until it ends.
pub(in super::super) struct PassedOn {
    body: Incoming,
    of: Of,
}

/// What an answer is the body of.
pub(super) enum Of {
    /// A stream of events, which ends once its server has moved or been removed.
    Events(Option<Gone>),
    /// A call, which counts until its answer is passed on.
    Call { _counted: Counted },
}

impl PassedOn {
    pub(super) fn new(body: Incoming, of: Of) -> Self {
        Self { body, of }
    }
}

/// Whether `asked` asks for a stream of the server's events.
pub(super) fn is_for_events(asked: &Asked<'_>) -> bool {
    let listens = || Message::of(&asked.body).is_some_and(|message| message.method == LISTEN);
    *asked.method == Method::GET || (*asked.method == Method::POST && listens())
}

impl Body for PassedOn {
    type Data = Bytes;
    type Error = hyper::Error;

    fn poll_frame(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        if let Of::Events(ends) = &mut self.of
            && ends.as_mut().is_some_and(|ends| ends.as_mut().poll(cx).is_ready())
        {
            *ends = None;
            return Poll::Ready(None);
        }
        Pin::new(&mut self.body).poll_frame(cx)
    }

    fn is_end_stream(&self) -> bool {
        self.body.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.body.size_hint()
    }
}
