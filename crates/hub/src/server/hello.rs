use crate::versions;
use protocol::Build;
use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, Hello, HelloReply, ReplyOutcome};
use protocol::versions::Versions;
pub(super) fn hello(build: Build, request: Option<&ClientRequest>) -> (ReplyOutcome, bool) {
    let error = |message: String| (ReplyOutcome::Error(ApiError::new(ErrorCode::VersionMismatch, message)), false);
    let Some(ClientRequest::Hello(Hello { protocol_version, oldest_protocol_version, .. })) = request else {
        return error("the first request on a connection must be hello".to_string());
    };
    let theirs = Versions::from_hello(*protocol_version, *oldest_protocol_version);
    match versions::CLIENTS.newest_shared(theirs) {
        Some(chosen) => (
            ReplyOutcome::Response(ClientResponse::Hello(HelloReply {
                protocol_version: chosen,
                hub_version: build.version().to_string(),
            })),
            true,
        ),
        None => error(format!(
            "this hub is version {} on {}, and this client speaks {theirs}. Stopping the hub, which also stops its agents, lets one that matches start",
            build.version(),
            versions::CLIENTS,
        )),
    }
}
