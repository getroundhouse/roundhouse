use serde::Deserialize;
use url::Url;

use crate::registering::NO_SECRET;

/// A protected server's metadata, as RFC 9728, section 2, has it, as far as a sign-in needs it.
#[derive(Deserialize)]
pub(super) struct ServerMetadata {
    pub(super) resource: String,
    #[serde(default)]
    pub(super) authorization_servers: Vec<String>,
    #[serde(default)]
    pub(super) scopes_supported: Option<Vec<String>>,
}

impl ServerMetadata {
    /// Whether it says it's of `server`. An address is the same with its host in capitals, with its scheme's own port,
    /// and with `/` for no path, as MCP's spec has a client take either, and otherwise letter for letter. But a server's
    /// query, which may only choose among its tools, can be left out, as some services leave it out of theirs. One with
    /// a query that isn't the server's is of another.
    pub(super) fn is_of(&self, server: &Url) -> bool {
        let without_query = || {
            let mut server = server.clone();
            server.set_query(None);
            server
        };
        Url::parse(&self.resource).is_ok_and(|resource| resource == *server || resource == without_query())
    }
}

/// An authorization server's metadata, as RFC 8414, section 2, has it, as far as a sign-in needs it.
#[derive(Deserialize)]
pub(super) struct AuthorizationServerMetadata {
    pub(super) issuer: String,
    pub(super) authorization_endpoint: String,
    pub(super) token_endpoint: String,
    #[serde(default)]
    pub(super) code_challenge_methods_supported: Option<Vec<String>>,
    #[serde(default)]
    pub(super) client_id_metadata_document_supported: Option<bool>,
    #[serde(default)]
    pub(super) registration_endpoint: Option<String>,
    #[serde(default)]
    pub(super) token_endpoint_auth_methods_supported: Option<Vec<String>>,
    #[serde(default)]
    pub(super) authorization_response_iss_parameter_supported: Option<bool>,
    #[serde(default)]
    pub(super) scopes_supported: Option<Vec<String>>,
}

impl AuthorizationServerMetadata {
    pub(super) fn takes_s256(&self) -> bool {
        self.code_challenge_methods_supported.iter().flatten().any(|method| method == "S256")
    }

    pub(super) fn takes_client_documents(&self) -> bool {
        self.client_id_metadata_document_supported == Some(true)
    }

    /// Whether it takes a client with no secret. One that doesn't say is tried, whatever RFC 8414, section 2, takes that
    /// for, since it says so as it registers one. *Unconfirmed.*
    pub(super) fn takes_clients_without_secrets(&self) -> bool {
        let methods = self.token_endpoint_auth_methods_supported.as_ref();
        methods.is_none_or(|methods| methods.iter().any(|method| method == NO_SECRET))
    }

    pub(super) fn says_iss(&self) -> bool {
        self.authorization_response_iss_parameter_supported == Some(true)
    }
}
