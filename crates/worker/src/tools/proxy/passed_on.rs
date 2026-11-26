use hyper::Method;
use hyper::body::{Body, Bytes, Frame, Incoming, SizeHint};
use std::pin::Pin;
use std::task::{Context, Poll};

type Moved = Pin<Box<dyn Future<Output = ()> + Send>>;

/// The body of a server's answer, as it comes. A server's stream of events, which a GET asks for, stays open for as
/// long as an agent is connected, so it ends as the server is moved to another URL. A call's answer goes on as it
/// began.
pub(in super::super) struct PassedOn {
    body: Incoming,
    ends: Option<Moved>,
}

impl PassedOn {
    /// The answer to a request by `method`, of a server that's moved once `moved` is done.
    pub(super) fn new(body: Incoming, method: &Method, moved: impl Future<Output = ()> + Send + 'static) -> Self {
        let ends: Option<Moved> = if *method == Method::GET { Some(Box::pin(moved)) } else { None };
        Self { body, ends }
    }
}

impl Body for PassedOn {
    type Data = Bytes;
    type Error = hyper::Error;

    fn poll_frame(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        if self.ends.as_mut().is_some_and(|ends| ends.as_mut().poll(cx).is_ready()) {
            self.ends = None;
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
