use http_body_util::{Either, Full};
use hyper::body::{Bytes, Incoming};
use hyper::header::{CONTENT_TYPE, HeaderValue};
use hyper::{Response, StatusCode};

/// What's answered with: something the worker has whole, or a tool server's answer, passed on as it comes.
pub(in super::super) type AnswerBody = Either<Full<Bytes>, Incoming>;

pub(super) fn empty(status: StatusCode) -> Response<AnswerBody> {
    let mut response = Response::new(Either::Left(Full::default()));
    *response.status_mut() = status;
    response
}

pub(in super::super) fn whole(status: StatusCode, content_type: HeaderValue, body: String) -> Response<AnswerBody> {
    let mut response = Response::new(Either::Left(Full::new(Bytes::from(body))));
    *response.status_mut() = status;
    response.headers_mut().insert(CONTENT_TYPE, content_type);
    response
}
