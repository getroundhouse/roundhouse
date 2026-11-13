//! Every request a client sends its hub, spelled out, and every reply the hub sends back, with at least its `result`,
//! since clients and hubs of different builds meet each other.

use protocol::client::{ClientFrame, ClientRequest, ClientResponse, HubFrame, ReplyOutcome};

use super::keeps_its_form;

mod replies;
mod requests;
mod session_requests;

/// `request` is written as `{"id":7,"request":<form>}`, and that reads as `request`.
fn asked(request: ClientRequest, form: &str) {
    keeps_its_form(&ClientFrame { id: 7, request }, &format!(r#"{{"id":7,"request":{form}}}"#));
}

/// `response` is written as a reply to call 7 whose response is `form`, and that reads as `response`.
fn answered(response: ClientResponse, form: &str) {
    let reply = HubFrame::Reply { id: 7, outcome: ReplyOutcome::Response(response) };
    keeps_its_form(&reply, &format!(r#"{{"frame":"reply","id":7,"response":{form}}}"#));
}
