use std::future::pending;

use http::header::WWW_AUTHENTICATE;
use serde_json::json;

use super::replies::{Reply, said, status};
use super::script::{MetadataAt, Says, Script, ServerMetadataAt};
use super::serving::Addresses;

/// Where its server's metadata is when its 401 says.
const SAID: &str = "/metadata";

/// A 401 that asks for a sign-in, as `script` has its server ask.
pub(super) fn unauthorized(script: &Script, at: &Addresses) -> Reply {
    let mut asks = vec![r#"Bearer realm="made up""#.to_string()];
    asks.extend(script.challenge_scope.map(|scope| format!("scope=\"{scope}\"")));
    if script.server_metadata_at == ServerMetadataAt::Said {
        asks.push(format!("resource_metadata=\"{}{SAID}\"", at.host));
    }
    let mut reply = status(401);
    reply.headers_mut().insert(WWW_AUTHENTICATE, asks.join(", ").parse().unwrap());
    reply
}

/// The metadata at `path`, if `script` has some there.
pub(super) async fn at(path: &str, script: &Script, at: &Addresses) -> Option<Reply> {
    let well_known = "/.well-known/oauth-protected-resource";
    let of_server = match script.server_metadata_at {
        ServerMetadataAt::Said => path == SAID,
        ServerMetadataAt::ItsPath => path == format!("{well_known}/mcp"),
        ServerMetadataAt::ItsHost => path == well_known,
        ServerMetadataAt::Answers(answered) if path.starts_with(well_known) => return Some(status(answered)),
        ServerMetadataAt::Nowhere | ServerMetadataAt::Answers(_) => false,
    };
    if of_server {
        return Some(of_the_server(script, at));
    }
    let issuer_path = script.issuer_path;
    let oauth = path == format!("/.well-known/oauth-authorization-server{issuer_path}");
    let openid = path == format!("/.well-known/openid-configuration{issuer_path}");
    let openid_appended = path == format!("{issuer_path}/.well-known/openid-configuration");
    let there = match script.metadata_at {
        MetadataAt::Oauth => oauth,
        MetadataAt::OpenId => openid,
        MetadataAt::OpenIdAppended => openid_appended,
        MetadataAt::Nowhere => false,
        MetadataAt::Answers(_) | MetadataAt::NeverAnswers if !(oauth || openid || openid_appended) => false,
        MetadataAt::Answers(answered) => return Some(status(answered)),
        MetadataAt::NeverAnswers => return pending().await,
    };
    there.then(|| of_the_authorization_server(script, at))
}

fn of_the_server(script: &Script, at: &Addresses) -> Reply {
    let resource = match script.resource {
        Says::Itself => at.resource(script),
        Says::Another => "https://other.example/mcp".to_string(),
    };
    let servers = [at.issuer(script)];
    said(
        200,
        &json!({"resource": resource, "authorization_servers": servers, "scopes_supported": script.server_scopes}),
    )
}

fn of_the_authorization_server(script: &Script, at: &Addresses) -> Reply {
    let issuer = match script.issuer {
        Says::Itself => at.issuer(script),
        Says::Another => "https://other.example".to_string(),
    };
    let mut metadata = json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{}/authorize?tenant=made-up", at.host),
        "token_endpoint": format!("{}/token", at.host),
        "response_types_supported": ["code"],
        "client_id_metadata_document_supported": script.client_documents,
        "authorization_response_iss_parameter_supported": script.iss.is_announced(),
        "scopes_supported": script.authorization_scopes,
    });
    if script.s256 {
        metadata["code_challenge_methods_supported"] = json!(["plain", "S256"]);
    }
    said(200, &metadata)
}
