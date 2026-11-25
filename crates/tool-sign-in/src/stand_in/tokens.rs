use std::collections::{HashMap, HashSet};

use base64::Engine as _;
use base64::prelude::BASE64_URL_SAFE_NO_PAD;
use ring::digest::{SHA256, digest};
use serde_json::json;

use super::Asked;
use super::replies::{Reply, oauth_error, said, status};
use super::script::{Refreshing, Script};

/// What the authorization server has given, and still takes.
#[derive(Default)]
pub(super) struct Tokens {
    given: usize,
    codes: HashMap<String, Approved>,
    access: HashSet<String>,
    /// Each refresh token, with whom it's for, and what.
    refresh: HashMap<String, (String, String)>,
}

/// What the page was asked for, once the person approved it.
pub(super) struct Approved {
    pub(super) challenge: String,
    pub(super) redirect: String,
    pub(super) client: String,
    pub(super) resource: String,
}

impl Tokens {
    /// A code for what the person approved, good once.
    pub(super) fn approve(&mut self, approved: Approved) -> String {
        let code = self.next("code");
        self.codes.insert(code.clone(), approved);
        code
    }

    /// Whether the MCP server takes `access`.
    pub(super) fn takes(&self, access: &str) -> bool {
        self.access.contains(access)
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
        let Some(approved) = self.codes.remove(&said("code")) else { return oauth_error("invalid_grant") };
        let hash = BASE64_URL_SAFE_NO_PAD.encode(digest(&SHA256, said("code_verifier").as_bytes()));
        let same = hash == approved.challenge && said("redirect_uri") == approved.redirect;
        if !same || said("client_id") != approved.client {
            return oauth_error("invalid_grant");
        }
        if said("resource") != approved.resource {
            return oauth_error("invalid_target");
        }
        let refresh = self.new_refresh(approved.client, approved.resource);
        self.give(Some(refresh), script)
    }

    fn for_refresh(&mut self, asked: &Asked, script: &Script) -> Reply {
        match script.refreshing {
            Refreshing::Works => {}
            Refreshing::Refused(error) => return oauth_error(error),
            Refreshing::Answers(answered) => return status(answered),
        }
        let said = |name: &str| asked.form(name).unwrap_or_default();
        let used = said("refresh_token");
        let Some((client, resource)) = self.refresh.get(&used).cloned() else { return oauth_error("invalid_grant") };
        if said("client_id") != client {
            return oauth_error("invalid_client");
        }
        if said("resource") != resource {
            return oauth_error("invalid_target");
        }
        let refresh = script.rotates.then(|| {
            self.refresh.remove(&used);
            self.new_refresh(client, resource)
        });
        self.give(refresh, script)
    }

    fn new_refresh(&mut self, client: String, resource: String) -> String {
        let refresh = self.next("refresh");
        self.refresh.insert(refresh.clone(), (client, resource));
        refresh
    }

    fn give(&mut self, refresh: Option<String>, script: &Script) -> Reply {
        let access = self.next("access");
        self.access.insert(access.clone());
        let mut given = json!({"access_token": access, "token_type": "Bearer"});
        if let Some(refresh) = refresh {
            given["refresh_token"] = refresh.into();
        }
        if let Some(lasts) = script.lasts {
            given["expires_in"] = lasts.into();
        }
        said(200, &given)
    }

    /// One given for the first time, such as `made-up-access-3`.
    fn next(&mut self, kind: &str) -> String {
        self.given += 1;
        format!("made-up-{kind}-{}", self.given)
    }
}
