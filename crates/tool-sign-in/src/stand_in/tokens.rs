use std::collections::HashMap;

use base64::Engine as _;
use base64::prelude::BASE64_URL_SAFE_NO_PAD;
use protocol::model::ToolScopes;
use ring::digest::{SHA256, digest};
use serde_json::json;

use super::Asked;
use super::replies::{Reply, oauth_error, said, status};
use super::script::{Refreshing, SaysAllowed, Script};

/// What the authorization server has given, and still takes.
#[derive(Default)]
pub(super) struct Tokens {
    given: usize,
    /// Each client that's registered, with where it's sent back to.
    clients: HashMap<String, Vec<String>>,
    codes: HashMap<String, Approved>,
    /// Each access token, with what it's allowed.
    access: HashMap<String, ToolScopes>,
    refresh: HashMap<String, Grant>,
}

/// Whether `client` is the address of a document.
fn has_document(client: &str) -> bool {
    client.starts_with("https://")
}

/// What the page was asked for, once the person approved it.
pub(super) struct Approved {
    pub(super) challenge: String,
    pub(super) redirect: String,
    pub(super) grant: Grant,
}

/// Whom a token is for, and what it's good for.
#[derive(Clone)]
pub(super) struct Grant {
    pub(super) client: String,
    pub(super) resource: String,
    pub(super) scopes: ToolScopes,
}

impl Tokens {
    /// The ID of a client that's sent back to `redirects`, now registered.
    pub(super) fn register(&mut self, redirects: Vec<String>) -> String {
        let client = self.next("client");
        self.clients.insert(client.clone(), redirects);
        client
    }

    /// Registers none of those that have been any more.
    pub(super) fn forget_clients(&mut self) {
        self.clients.clear();
    }

    /// Whether `client` is one it knows, as its document's address is of any that has one.
    fn knows(&self, client: &str) -> bool {
        has_document(client) || self.clients.contains_key(client)
    }

    /// Whether `client` is one its page sends a person back to `redirect` for.
    pub(super) fn sends_back(&self, client: &str, redirect: &str, script: &Script) -> bool {
        if has_document(client) {
            return script.client_documents;
        }
        self.clients.get(client).is_some_and(|redirects| redirects.iter().any(|one| one == redirect))
    }

    /// A code for what the person approved, good once.
    pub(super) fn approve(&mut self, approved: Approved) -> String {
        let code = self.next("code");
        self.codes.insert(code.clone(), approved);
        code
    }

    /// What the MCP server allows `access`, if it takes it.
    pub(super) fn allowed(&self, access: &str) -> Option<ToolScopes> {
        self.access.get(access).cloned()
    }

    /// Takes no access token it has given so far, as once they've ended.
    pub(super) fn end_access(&mut self) {
        self.access.clear();
    }

    /// What `POST /token` answers `asked`.
    pub(super) fn answer(&mut self, asked: &Asked, script: &Script) -> Reply {
        match asked.form("grant_type").as_deref() {
            Some("authorization_code") => self.for_code(asked, script),
            Some("refresh_token") => self.for_refresh(asked, script),
            _ => oauth_error("unsupported_grant_type"),
        }
    }

    fn for_code(&mut self, asked: &Asked, script: &Script) -> Reply {
        let said = |name: &str| asked.form(name).unwrap_or_default();
        if !self.knows(&said("client_id")) {
            return oauth_error("invalid_client");
        }
        let Some(approved) = self.codes.remove(&said("code")) else { return oauth_error("invalid_grant") };
        let hash = BASE64_URL_SAFE_NO_PAD.encode(digest(&SHA256, said("code_verifier").as_bytes()));
        let same = hash == approved.challenge && said("redirect_uri") == approved.redirect;
        if !same || said("client_id") != approved.grant.client {
            return oauth_error("invalid_grant");
        }
        if said("resource") != approved.grant.resource {
            return oauth_error("invalid_target");
        }
        let refresh = self.new_refresh(approved.grant.clone());
        self.give(approved.grant.scopes, Some(refresh), script)
    }

    fn for_refresh(&mut self, asked: &Asked, script: &Script) -> Reply {
        match script.refreshing {
            Refreshing::Works => {}
            Refreshing::Refused(error) => return oauth_error(error),
            Refreshing::Answers(answered) => return status(answered),
        }
        let said = |name: &str| asked.form(name).unwrap_or_default();
        let used = said("refresh_token");
        let Some(grant) = self.refresh.get(&used).cloned() else { return oauth_error("invalid_grant") };
        if said("client_id") != grant.client || !self.knows(&grant.client) {
            return oauth_error("invalid_client");
        }
        if said("resource") != grant.resource {
            return oauth_error("invalid_target");
        }
        let refresh = script.rotates.then(|| {
            self.refresh.remove(&used);
            self.new_refresh(grant.clone())
        });
        self.give(grant.scopes, refresh, script)
    }

    fn new_refresh(&mut self, grant: Grant) -> String {
        let refresh = self.next("refresh");
        self.refresh.insert(refresh.clone(), grant);
        refresh
    }

    fn give(&mut self, scopes: ToolScopes, refresh: Option<String>, script: &Script) -> Reply {
        let access = self.next("access");
        let allowed = script.allows.map_or(scopes, |allows| ToolScopes::of(allows.split(' ')));
        let mut given = json!({"access_token": &access, "token_type": "Bearer"});
        if let Some(refresh) = refresh {
            given["refresh_token"] = refresh.into();
        }
        if let Some(lasts) = script.lasts {
            given["expires_in"] = lasts.into();
        }
        match &script.says_allowed {
            SaysAllowed::Nothing => {}
            SaysAllowed::WhatItAllows => given["scope"] = allowed.to_string().into(),
            SaysAllowed::Else(said) => given["scope"] = said.clone(),
        }
        self.access.insert(access, allowed);
        said(200, &given)
    }

    /// One given for the first time, such as `made-up-access-3`.
    fn next(&mut self, kind: &str) -> String {
        self.given += 1;
        format!("made-up-{kind}-{}", self.given)
    }
}
