use crate::versions;
use protocol::Build;
use protocol::client::{
    ApiError, ClientRequest, ClientResponse, ErrorCode, Hello, HelloReply, HubRefusing, ReplyOutcome,
};
use protocol::versions::Versions;
pub(super) fn hello(build: Build, request: Option<&ClientRequest>) -> (ReplyOutcome, bool) {
    let Some(ClientRequest::Hello(Hello { protocol_version, oldest_protocol_version, .. })) = request else {
        let message = "the first request on a connection must be hello";
        return (ReplyOutcome::Error(ApiError::new(ErrorCode::VersionMismatch, message)), false);
    };
    let theirs = Versions::from_hello(*protocol_version, *oldest_protocol_version);
    let Some(chosen) = versions::CLIENTS.newest_shared(theirs) else {
        let message = format!(
            "this hub is version {} on {}, and this client speaks {theirs}. Stopping the hub, which also stops its agents, lets one that matches start",
            build.version(),
            versions::CLIENTS,
        );
        let hub = HubRefusing { version: build.version().to_string(), speaks: versions::CLIENTS };
        return (ReplyOutcome::Error(ApiError::versions_differ(message, hub)), false);
    };
    let reply = HelloReply { protocol_version: chosen, hub_version: build.version().to_string() };
    (ReplyOutcome::Response(ClientResponse::Hello(reply)), true)
}
