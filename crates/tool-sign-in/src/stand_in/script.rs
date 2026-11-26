/// How the stand-in answers. By default it asks for a sign-in as MCP has a server do, the person approves, and each
/// renewal gives a new refresh token.
#[derive(Clone, Debug)]
pub struct Script {
    pub asks: Asks,
    /// The MCP method its server refuses with a 400, before it asks for a sign-in, as one that takes only other versions
    /// of MCP may.
    pub refuses_method: Option<&'static str>,
    pub server_metadata_at: ServerMetadataAt,
    /// What its 401 says to ask to be allowed.
    pub challenge_scope: Option<&'static str>,
    /// What its server's metadata says to.
    pub server_scopes: Option<Vec<&'static str>>,
    /// Who its server's metadata says it's of.
    pub resource: Says,
    /// The path its authorization server's name ends in, such as `/tenant`, or nothing.
    pub issuer_path: &'static str,
    pub metadata_at: MetadataAt,
    /// Who its authorization server's metadata says it's of.
    pub issuer: Says,
    /// Whether its authorization server takes client documents.
    pub client_documents: bool,
    /// Whether its authorization server says it takes S256.
    pub s256: bool,
    /// What its authorization server says it can allow.
    pub authorization_scopes: Option<Vec<&'static str>>,
    pub iss: Iss,
    pub person: Person,
    /// How many seconds each access token is said to last.
    pub lasts: Option<u64>,
    /// Whether each renewal gives a new refresh token, with the last refused from then on.
    pub rotates: bool,
    pub refreshing: Refreshing,
}

impl Default for Script {
    fn default() -> Self {
        Self {
            asks: Asks::SignIn,
            refuses_method: None,
            server_metadata_at: ServerMetadataAt::Said,
            challenge_scope: Some("read write"),
            server_scopes: None,
            resource: Says::Itself,
            issuer_path: "",
            metadata_at: MetadataAt::Oauth,
            issuer: Says::Itself,
            client_documents: true,
            s256: true,
            authorization_scopes: None,
            iss: Iss::Said,
            person: Person::Approves,
            lasts: Some(3600),
            rotates: true,
            refreshing: Refreshing::Works,
        }
    }
}

/// Whether its server asks for a sign-in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asks {
    SignIn,
    /// It answers anyone.
    Nothing,
    /// It answers this status to anyone, such as 503 while it's down, or 307, which says it's at `/moved`.
    Answers(u16),
}

/// Where its server's metadata is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServerMetadataAt {
    /// Where its 401 says, which is none of the well-known places.
    Said,
    /// The well-known place for its path.
    ItsPath,
    /// The well-known place for its host, which is what the metadata there is of.
    ItsHost,
    Nowhere,
    /// Nowhere, with this status at the well-known places, such as 503.
    Answers(u16),
}

/// Where its authorization server's metadata is, of the places MCP has a client look.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetadataAt {
    Oauth,
    OpenId,
    /// Where OIDC's own spec has it, for a name that ends in a path.
    OpenIdAppended,
    Nowhere,
    /// Nowhere, with this status at each.
    Answers(u16),
    /// Nowhere, with no answer at any.
    NeverAnswers,
}

/// Who metadata says it's of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Says {
    Itself,
    Another,
}

/// Who its authorization server says it is as it sends the person back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Iss {
    /// Itself, as its metadata says it will.
    Said,
    /// Itself, though its metadata doesn't say it will.
    SaidUnannounced,
    /// Nothing, as its metadata says.
    Unsaid,
    /// Nothing, though its metadata says it will.
    Missing,
    /// Another, as its metadata says it will.
    Another,
    /// Another, though its metadata doesn't say it will.
    AnotherUnannounced,
}

impl Iss {
    pub(super) fn is_announced(self) -> bool {
        matches!(self, Iss::Said | Iss::Missing | Iss::Another)
    }
}

/// What the person does on the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Person {
    Approves,
    Refuses,
}

/// What its authorization server does with a refresh token it would take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refreshing {
    Works,
    /// Refuses it with this of OAuth's errors, such as `invalid_grant`.
    Refused(&'static str),
    /// Answers this status, with no more, such as 503, or 307, which says it's at `/moved`.
    Answers(u16),
}
