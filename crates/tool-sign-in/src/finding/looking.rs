use std::time::Duration;

use protocol::client::tool_sign_ins::NoToolSignIn;
use protocol::model::{ToolScopes, ToolServerUrl};
use tokio::time::{Instant, timeout_at};
use tool_server_http::Challenge;
use url::Url;

use super::asking::as_nobody;
use super::metadata::{AuthorizationServerMetadata, ServerMetadata};
use super::reading::{Read, authorization_server_metadata, server_metadata};
use super::{Found, Issuer, Takes, Way};
use crate::failure::{host_of, shown};
use crate::fetch::Caller;
use crate::fetch::url_of;
use crate::offline_access::lists_offline;
use crate::{Failure, ToolSignIn};

/// How long finding out has in all, which is several calls, each of which is answered in well under a second.
pub(super) const FIND_WITHIN: Duration = Duration::from_secs(30);

/// What a server and its authorization server say of signing in.
enum Said {
    /// How to, or why Roundhouse can't.
    Found(Found),
    Nothing,
    /// It couldn't be read whether they say.
    Unread(Failure),
}

impl ToolSignIn {
    /// Finds out whether `server` takes a sign-in, and where. One that answers anyone may have one too, which its
    /// metadata says, so it's looked at for that, and has none where that's not there or can't be read.
    pub async fn find(&self, server: &ToolServerUrl) -> Result<Found, Failure> {
        let by = Instant::now() + FIND_WITHIN;
        let url = url_of(server)?;
        let timed_out = || Failure::TimedOut { host: url.host_str().unwrap_or_default().to_string() };
        let asking = async {
            let calls = self.calls_for(&url).await?;
            let challenge = as_nobody(&calls, &url).await?;
            Ok((calls, challenge))
        };
        let (calls, asked) = timeout_at(by, asking).await.unwrap_or_else(|_| Err(timed_out()))?;
        let asks = asked.is_some();
        let looking = look(&calls, server, &url, asked.unwrap_or_default());
        match timeout_at(by, looking).await.unwrap_or_else(|_| Ok(Said::Unread(timed_out())))? {
            Said::Found(found) => Ok(found),
            Said::Nothing | Said::Unread(_) if !asks => Ok(Found::NoSignIn),
            Said::Nothing => Ok(Found::NoWay(NoToolSignIn::NoAuthorizationServer)),
            Said::Unread(failure) => Err(failure),
        }
    }
}

/// What `server`'s metadata says, and its authorization server's, with `url` its address. A failure is of what's there
/// and can't be right.
async fn look(calls: &Caller<'_>, server: &ToolServerUrl, url: &Url, challenge: Challenge) -> Result<Said, Failure> {
    let metadata = match server_metadata(calls, url, &challenge).await? {
        Read::Right(metadata) => metadata,
        Read::OfAnother(said) => {
            let host = host_of(&said).unwrap_or_default();
            return Ok(Said::Found(Found::NoWay(NoToolSignIn::OtherServer { host })));
        }
        Read::Nothing => return Ok(Said::Nothing),
        Read::Unread(failure) => return Ok(Said::Unread(failure)),
    };
    // Which of several is the client's choice, and the first is the one the server puts first.
    let Some(issuer) = metadata.authorization_servers.first() else { return Ok(Said::Nothing) };
    match authorization_server_metadata(calls, issuer).await? {
        Read::Right(of_issuer) => Ok(Said::Found(way(calls, server, challenge, metadata, of_issuer)?)),
        Read::OfAnother(said) => Err(Failure::OtherIssuer { asked: shown(issuer), host: host_of(&said) }),
        Read::Nothing => Ok(Said::Nothing),
        Read::Unread(failure) => Ok(Said::Unread(failure)),
    }
}

/// How to sign in, unless the authorization server takes no client like Roundhouse.
fn way(
    calls: &Caller<'_>,
    server: &ToolServerUrl,
    challenge: Challenge,
    metadata: ServerMetadata,
    authorization_server: AuthorizationServerMetadata,
) -> Result<Found, Failure> {
    let issuer = Issuer::new(&authorization_server.issuer);
    let at = || shown(issuer.as_str());
    let takes = match (&authorization_server.registration_endpoint, authorization_server.takes_client_documents()) {
        (_, true) => Takes::Documents,
        (None, false) => return Ok(Found::NoWay(NoToolSignIn::OnlyRegisteredClients { authorization_server: at() })),
        (Some(_), false) if !authorization_server.takes_clients_without_secrets() => {
            return Ok(Found::NoWay(NoToolSignIn::OnlyClientsWithSecrets { authorization_server: at() }));
        }
        (Some(endpoint), false) => Takes::Registered { endpoint: calls.address(endpoint)? },
    };
    if !authorization_server.takes_s256() {
        return Ok(Found::NoWay(NoToolSignIn::NoS256 { authorization_server: at() }));
    }
    let says_iss = authorization_server.says_iss();
    let scopes = challenge.scope.unwrap_or_else(|| ToolScopes::of(metadata.scopes_supported.unwrap_or_default()));
    let offline = lists_offline(&authorization_server.scopes_supported.unwrap_or_default());
    Ok(Found::SignIn(Box::new(Way {
        server: server.clone(),
        resource: metadata.resource,
        issuer,
        authorization_endpoint: calls.address(&authorization_server.authorization_endpoint)?,
        token_endpoint: calls.address(&authorization_server.token_endpoint)?,
        takes,
        says_iss,
        scopes,
        offline,
    })))
}
