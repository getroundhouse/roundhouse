use serde_json::Value;
use waiting::Gate;

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
    /// What its server refuses calls for the lack of, with a 403 that says so.
    pub wants: Vec<Wants>,
    /// What its server's metadata says to.
    pub server_scopes: Option<Vec<&'static str>>,
    /// The query its server's address has.
    pub query: Query,
    /// Who its server's metadata says it's of.
    pub resource: Says,
    /// The path its authorization server's name ends in, such as `/tenant`, or nothing.
    pub issuer_path: &'static str,
    pub metadata_at: MetadataAt,
    /// Who its authorization server's metadata says it's of.
    pub issuer: Says,
    /// Whether its authorization server takes client documents.
    pub client_documents: bool,
    /// What its authorization server does with a client that asks to be registered.
    pub registers: Registers,
    /// How its authorization server says a client can say who it is for tokens.
    pub client_auth_methods: Option<Vec<&'static str>>,
    /// Whether its authorization server says it takes S256.
    pub s256: bool,
    /// What its authorization server says it can allow.
    pub authorization_scopes: Option<Vec<&'static str>>,
    pub iss: Iss,
    pub person: Person,
    /// What its authorization server allows, whatever it's asked for. `None` where that's what it's asked for.
    pub allows: Option<&'static str>,
    /// What its authorization server says it allows as it gives tokens.
    pub says_allowed: SaysAllowed,
    /// How many seconds each access token is said to last.
    pub lasts: Option<u64>,
    /// Whether each renewal gives a new refresh token, with the last refused from then on.
    pub rotates: bool,
    pub refreshing: Refreshing,
    /// What holds back each piece of its metadata.
    pub metadata_after: Option<Gate>,
    /// What holds back each answer of its authorization server for tokens, and to a client that asks to be registered,
    /// once it has done what it was asked, as a refresh token that's replaced is already refused.
    pub answers_after: Option<Gate>,
}

impl Default for Script {
    fn default() -> Self {
        Self {
            asks: Asks::SignIn,
            refuses_method: None,
            server_metadata_at: ServerMetadataAt::Said,
            challenge_scope: Some("read write"),
            wants: vec![],
            server_scopes: None,
            query: Query::None,
            resource: Says::Itself,
            issuer_path: "",
            metadata_at: MetadataAt::Oauth,
            issuer: Says::Itself,
            client_documents: true,
            registers: Registers::Nobody,
            client_auth_methods: None,
            s256: true,
            authorization_scopes: None,
            iss: Iss::Said,
            person: Person::Approves,
            allows: None,
            says_allowed: SaysAllowed::Nothing,
            lasts: Some(3600),
            rotates: true,
            refreshing: Refreshing::Works,
            metadata_after: None,
            answers_after: None,
        }
    }
}

/// The scope a token has to be allowed for a call of `tool`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wants {
    pub tool: &'static str,
    pub scope: &'static str,
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

/// The query a server's address has, such as `tools=a`, and whether its metadata says it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Query {
    None,
    Repeated(&'static str),
    LeftOut(&'static str),
}

/// Who metadata says it's of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Says {
    Itself,
    /// Its host alone.
    ItsHost,
    Another,
}

/// What its authorization server does with a client that asks to be registered.
#[derive(Clone, Debug, PartialEq)]
pub enum Registers {
    /// It has nowhere to ask.
    Nobody,
    /// Registers each that asks, at `/register`, with an ID, and with a secret and a token to change it with besides.
    Anyone,
    /// Registers nobody, and answers with this status and body.
    Says(u16, Value),
    /// It says the place to ask is this address, which isn't its own.
    At(String),
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

/// What its authorization server says it allows as it gives tokens.
#[derive(Clone, Debug, PartialEq)]
pub enum SaysAllowed {
    Nothing,
    WhatItAllows,
    /// This, which may be no scopes at all.
    Else(Value),
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
