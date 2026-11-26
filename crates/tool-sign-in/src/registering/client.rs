use protocol::client::tool_sign_ins::NoToolSignIn;
use url::Url;

use super::answer::{Answered, answered};
use super::{Registration, client_metadata};
use crate::failure::host_of;
use crate::fetch::url_of;
use crate::finding::Takes;
use crate::{ClientId, Failure, RedirectUri, ToolSignIn, Way};

/// The client Roundhouse is to an authorization server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Client {
    /// The one its document describes.
    Document,
    /// The one that was registered there.
    Registered(Registration),
}

/// What asking for a client to sign in as came to.
#[derive(Debug)]
pub enum TakenAs {
    Client(Client),
    /// The authorization server takes none that Roundhouse could be.
    NoWay(NoToolSignIn),
}

impl ToolSignIn {
    /// The client to sign in as by `way`, with the person sent back to `redirect`: the one its document describes, where
    /// that's taken, or `before`, where it's registered for both, and otherwise one that's registered now. So it's asked
    /// for only once the person has chosen to sign in.
    pub async fn client_for(
        &self,
        way: &Way,
        redirect: &RedirectUri,
        before: Option<&Registration>,
    ) -> Result<TakenAs, Failure> {
        let endpoint = match &way.takes {
            Takes::Documents => return Ok(TakenAs::Client(Client::Document)),
            Takes::Registered { endpoint } => endpoint,
        };
        match before.filter(|before| before.is_for(&way.issuer, redirect)) {
            Some(before) => Ok(TakenAs::Client(Client::Registered(before.clone()))),
            None => self.register(way, endpoint, redirect).await,
        }
    }

    async fn register(&self, way: &Way, endpoint: &Url, redirect: &RedirectUri) -> Result<TakenAs, Failure> {
        let mut redirect_uris = RedirectUri::listened_at().to_vec();
        if !redirect_uris.contains(redirect) {
            redirect_uris.push(redirect.clone());
        }
        let calls = self.calls_for(&url_of(&way.server)?).await?;
        let answer = calls.post_json(endpoint, &client_metadata(&redirect_uris)).await?;
        let (issuer, host) = (way.issuer.clone(), host_of(way.issuer.as_str()).unwrap_or_default());
        Ok(match answered(answer)? {
            Answered::Registered(client_id) => {
                TakenAs::Client(Client::Registered(Registration { client_id, issuer, redirect_uris }))
            }
            Answered::WithSecret => TakenAs::NoWay(NoToolSignIn::OnlyClientsWithSecrets { host }),
            Answered::Refused(error) => {
                let error = error.map(|error| error.to_string());
                TakenAs::NoWay(NoToolSignIn::NotRegistered { host, error })
            }
        })
    }

    /// What `client` is called.
    pub(crate) fn id_of<'a>(&'a self, client: &'a Client) -> &'a ClientId {
        match client {
            Client::Document => &self.document,
            Client::Registered(registration) => &registration.client_id,
        }
    }
}

impl Client {
    /// The registration that made it, where one did.
    pub fn registration(&self) -> Option<&Registration> {
        match self {
            Client::Document => None,
            Client::Registered(registration) => Some(registration),
        }
    }
}
