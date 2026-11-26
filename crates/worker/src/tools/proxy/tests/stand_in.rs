use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll};

use http_body_util::{Either, Full};
use hyper::Response;
use hyper::body::{Body, Bytes, Frame};
use protocol::model::{ToolServerName, ToolServerUrl};
use protocol::worker::ProxiedServer;
use tokio::sync::mpsc;
use tool_server_http::StandIn;

/// A body sent as it's handed over.
pub(super) struct Events(pub(super) mpsc::Receiver<Bytes>);

impl Body for Events {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        self.0.poll_recv(cx).map(|sent| sent.map(|bytes| Ok(Frame::data(bytes))))
    }
}

pub(super) type Reply = Response<Either<Full<Bytes>, Events>>;

/// `server` as the hub sets it, with a query no agent may be given either, and its key.
pub(super) fn as_set(server: &StandIn) -> ProxiedServer {
    ProxiedServer {
        id: "t-1".into(),
        name: "linear_northwind".into(),
        label: ToolServerName::typed("Linear (Northwind)").unwrap(),
        url: server.url(),
        header: Some(StandIn::key()),
        sign_in: None,
    }
}

pub(super) fn url(text: &str) -> ToolServerUrl {
    ToolServerUrl::typed(text).unwrap()
}
