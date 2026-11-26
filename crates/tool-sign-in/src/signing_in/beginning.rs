use base64::Engine as _;
use base64::prelude::BASE64_URL_SAFE_NO_PAD;
use protocol::model::ToolScopes;
use ring::digest::{SHA256, digest};
use ring::rand::{SecureRandom as _, SystemRandom};

use super::RedirectUri;
use crate::offline_access::{has_offline, offline};
use crate::secrets::Verifier;
use crate::{AuthorizationPage, Client, Failure, Registration, SignInState, ToolSignIn, Way};

/// A sign-in the person hasn't approved yet.
#[derive(Debug)]
pub struct Started {
    /// The page to open in their browser.
    pub page: AuthorizationPage,
    /// What finishes it, kept in memory until it does.
    pub pending: Pending,
}

/// What a sign-in is finished with, once the person is sent back. It has the issuer as its metadata had it, checked, as
/// MCP has a client record it before the person is sent anywhere.
#[derive(Debug)]
pub struct Pending {
    pub(super) way: Way,
    pub(super) client: Client,
    pub(super) redirect: RedirectUri,
    pub(super) scopes: ToolScopes,
    pub(super) state: SignInState,
    pub(super) verifier: Verifier,
}

impl Pending {
    /// The registration it's signing in with, where it's with one.
    pub fn registration(&self) -> Option<&Registration> {
        self.client.registration()
    }
}

impl ToolSignIn {
    /// Starts signing in by `way` as `client`, which [`ToolSignIn::client_for`] gives, asking to be allowed `scopes`: the
    /// way's own, and those asked for before and wanted since where it's signing in again. What asks for a refresh token
    /// is asked for as well, where the authorization server says it has that. The person is sent back to `redirect`.
    /// Nothing is sent.
    pub fn begin(
        &self,
        way: Way,
        client: Client,
        scopes: ToolScopes,
        redirect: RedirectUri,
    ) -> Result<Started, Failure> {
        let scopes = if way.offline { offline(scopes) } else { scopes };
        let (state, verifier) = (SignInState::new(random()?), Verifier::new(random()?));
        let mut page = way.authorization_endpoint.clone();
        {
            let mut asks = page.query_pairs_mut();
            asks.append_pair("response_type", "code")
                .append_pair("client_id", self.id_of(&client).as_str())
                .append_pair("redirect_uri", redirect.as_str())
                .append_pair("state", state.as_str())
                .append_pair("code_challenge", &challenge_of(&verifier))
                .append_pair("code_challenge_method", "S256")
                .append_pair("resource", &way.resource);
            if !scopes.is_empty() {
                asks.append_pair("scope", &scopes.to_string());
            }
            // OIDC gives a refresh token only to someone asked to consent, by Core 1.0, section 11.
            if has_offline(&scopes) {
                asks.append_pair("prompt", "consent");
            }
        }
        let page = AuthorizationPage::new(page);
        Ok(Started { page, pending: Pending { way, client, redirect, scopes, state, verifier } })
    }
}

/// What the authorization server is sent of the verifier, its hash, as RFC 7636, section 4.2, makes it with S256.
pub(super) fn challenge_of(verifier: &Verifier) -> String {
    BASE64_URL_SAFE_NO_PAD.encode(digest(&SHA256, verifier.as_str().as_bytes()))
}

/// 256 bits from the system's source of randomness, in 43 characters that a URL takes, as RFC 7636, section 4.1, has a
/// verifier.
fn random() -> Result<String, Failure> {
    let mut bytes = [0_u8; 32];
    SystemRandom::new().fill(&mut bytes).map_err(|_| Failure::NoRandomness)?;
    Ok(BASE64_URL_SAFE_NO_PAD.encode(bytes))
}
