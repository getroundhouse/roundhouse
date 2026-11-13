use super::super::keeps_its_form;
use protocol::client::{ClientFrame, ClientRequest, ClientResponse, HubFrame, ReplyOutcome};

/// `request` is written as `{"id":7,"request":<form>}`, and that reads as `request`.
pub(super) fn asked(request: ClientRequest, form: &str) {
    keeps_its_form(&ClientFrame { id: 7, request }, &format!(r#"{{"id":7,"request":{form}}}"#));
}

/// `response` is written as a reply to call 7 whose response is `form`, and that reads as `response`.
pub(super) fn answered(response: ClientResponse, form: &str) {
    let reply = HubFrame::Reply { id: 7, outcome: ReplyOutcome::Response(response) };
    keeps_its_form(&reply, &format!(r#"{{"frame":"reply","id":7,"response":{form}}}"#));
}
