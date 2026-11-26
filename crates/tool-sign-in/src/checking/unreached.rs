use std::error::Error;
use std::io::ErrorKind;
use std::{io, iter};

use hyper_util::client::legacy;
use protocol::client::tool_server_checks::{NoToolList, ToolServerUnreached};

/// What it came to that a request wasn't answered, for `error`, whose words are the system's and never the server's.
pub(super) fn unreached(error: &(dyn Error + 'static)) -> NoToolList {
    // The only other is that the certificates couldn't be read.
    let Some(sending) = error.downcast_ref::<legacy::Error>() else {
        return NoToolList::Unreached { why: Some(ToolServerUnreached::NoCertificates) };
    };
    if !sending.is_connect() {
        return NoToolList::Lost;
    }
    for cause in causes(error) {
        if let Some(tls) = cause.downcast_ref::<rustls::Error>() {
            return NoToolList::Unreached { why: not_agreed(tls) };
        }
        match cause.downcast_ref::<io::Error>().map(io::Error::kind) {
            Some(ErrorKind::ConnectionRefused) => {
                return NoToolList::Unreached { why: Some(ToolServerUnreached::ConnectionRefused) };
            }
            Some(ErrorKind::HostUnreachable | ErrorKind::NetworkUnreachable) => {
                return NoToolList::Unreached { why: Some(ToolServerUnreached::NoRoute) };
            }
            Some(ErrorKind::TimedOut) => return NoToolList::TimedOut,
            _ => {}
        }
    }
    NoToolList::Unreached { why: None }
}

/// Why TLS wasn't agreed on.
fn not_agreed(tls: &rustls::Error) -> Option<ToolServerUnreached> {
    if let rustls::Error::InvalidCertificate(_) = tls {
        return Some(ToolServerUnreached::NotTrusted);
    }
    if let rustls::Error::InvalidMessage(_) = tls {
        return Some(ToolServerUnreached::NotHttps);
    }
    None
}

/// `error` and what caused it, each in turn. An `io::Error` made of another error is that error's own cause.
fn causes<'a>(error: &'a (dyn Error + 'static)) -> impl Iterator<Item = &'a (dyn Error + 'static)> {
    iter::successors(Some(error), |error| {
        let error: &'a (dyn Error + 'static) = *error;
        match error.downcast_ref::<io::Error>().and_then(io::Error::get_ref) {
            Some(made_of) => Some(made_of),
            None => error.source(),
        }
    })
}
