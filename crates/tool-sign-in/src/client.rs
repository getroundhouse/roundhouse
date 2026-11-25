use tokio::sync::OnceCell;

use crate::fetch::{Names, Tls};

/// Roundhouse's client document, whose address is what Roundhouse is to every authorization server, as its
/// `client_id`. It can't move while a sign-in made with it lasts.
pub const CLIENT_DOCUMENT: &str = "https://getroundhouse.dev/oauth/client.json";

/// Roundhouse as an OAuth client of MCP servers.
pub struct ToolSignIn {
    /// The address of its client document.
    pub(crate) document: String,
    pub(crate) names: Names,
    /// Made on the first call over TLS, since it reads the system's certificates.
    pub(crate) tls: OnceCell<Tls>,
}

impl ToolSignIn {
    /// The client whose document is at `document`, which is [`CLIENT_DOCUMENT`] but in a test.
    pub fn new(document: impl Into<String>) -> Self {
        Self { document: document.into(), names: Names::System, tls: OnceCell::new() }
    }
}
