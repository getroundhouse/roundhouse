use tool_server_http::{Names, Tls};

use crate::ClientId;

/// Roundhouse's client document, whose address is what Roundhouse is to every authorization server that takes such
/// documents, as its `client_id`. It can't move while a sign-in made with it lasts.
pub const CLIENT_DOCUMENT: &str = "https://getroundhouse.dev/oauth/client.json";

/// Roundhouse as a client of MCP servers, and of the authorization servers they're signed in to at.
pub struct ToolSignIn {
    /// What it's known as where its client document is taken, which is the document's address.
    pub(crate) document: ClientId,
    pub(crate) names: Names,
    pub(crate) tls: Tls,
}

impl ToolSignIn {
    /// The client whose document is at `document`, which is [`CLIENT_DOCUMENT`] but in a test.
    pub fn new(document: impl Into<String>) -> Self {
        Self { document: ClientId::of_document(document), names: Names::System, tls: Tls::default() }
    }
}
