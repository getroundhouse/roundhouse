use super::{Pending, Returned};
use crate::fetch::{json, url_of};
use crate::token_endpoint::{Refusal, granted};
use crate::{AuthorizationCode, Failure, Granted, Registration, Renewable, ToolSignIn};

/// What OAuth calls the person's own refusal.
const ACCESS_DENIED: &str = "access_denied";

/// A sign-in the person approved.
#[derive(Debug)]
pub struct SignedIn {
    /// What renews it, with its refresh token, which goes in a file of its own.
    pub renewable: Renewable,
    pub granted: Granted,
    /// The registration it was made with, where it was with one, for signing in again.
    pub registration: Option<Registration>,
}

impl ToolSignIn {
    /// Finishes `pending` with what the person was sent back with, for tokens, once it's seen to be of this sign-in and
    /// from the authorization server that was asked. [`Failure::WrongState`] says nothing of this sign-in, which is
    /// still waiting. Any other failure ends it, since a code is good once.
    pub async fn finish(&self, pending: &Pending, returned: Returned) -> Result<SignedIn, Failure> {
        let code = pending.code(returned)?;
        let Pending { way, client, redirect, scopes, verifier, .. } = pending;
        let client_id = self.id_of(client);
        let form = [
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", redirect.as_str()),
            ("client_id", client_id.as_str()),
            ("code_verifier", verifier.as_str()),
            ("resource", &way.resource),
        ];
        let calls = self.calls_for(&url_of(&way.server)?).await?;
        let answer = calls.post_form(&way.token_endpoint, &form).await?;
        if let Some(Refusal { error }) = json(&answer).filter(|_| answer.refused()) {
            return Err(Failure::NoTokens(error));
        }
        let renewable = Renewable {
            server: way.server.clone(),
            resource: way.resource.clone(),
            issuer: way.issuer.clone(),
            token_endpoint: way.token_endpoint.clone().into(),
            client_id: client_id.clone(),
            scopes: scopes.clone(),
        };
        Ok(SignedIn { renewable, granted: granted(answer)?, registration: client.registration().cloned() })
    }
}

impl Failure {
    /// Of what [`ToolSignIn::finish`] failed with, whether the person was sent back from the authorization server's page
    /// with the sign-in's own state, and from the authorization server that was asked, which its page does only for a
    /// client it knows. That's each failure but those of what came back, since finishing checks that first.
    pub fn was_sent_back(&self) -> bool {
        match self {
            Failure::WrongState | Failure::WrongIssuer => false,
            Failure::Unanswered { .. }
            | Failure::TimedOut { .. }
            | Failure::Busy { .. }
            | Failure::Answered { .. }
            | Failure::Unreadable { .. }
            | Failure::NotFetched { .. }
            | Failure::OtherIssuer { .. }
            | Failure::RefusedByPerson
            | Failure::RefusedAtPage(_)
            | Failure::NoTokens(_)
            | Failure::NoRandomness => true,
        }
    }
}

impl Pending {
    /// The code in `returned`, once its state and its issuer are right, before which nothing in it is believed, not
    /// even that the sign-in was refused, as MCP has a client do.
    fn code(&self, returned: Returned) -> Result<AuthorizationCode, Failure> {
        let Returned { code, state, iss, error, repeated } = returned;
        if repeated || state.as_ref() != Some(&self.state) {
            return Err(Failure::WrongState);
        }
        // RFC 9207, section 2.4: letter for letter, whether or not it said it would send it, and there if it did.
        let from_issuer = match iss {
            Some(iss) => iss == self.way.issuer.as_str(),
            None => !self.way.says_iss,
        };
        if !from_issuer {
            return Err(Failure::WrongIssuer);
        }
        match (error, code) {
            (Some(error), _) if error.as_str() == ACCESS_DENIED => Err(Failure::RefusedByPerson),
            (Some(error), _) => Err(Failure::RefusedAtPage(error)),
            (None, Some(code)) => Ok(code),
            (None, None) => {
                let host = self.way.authorization_endpoint.host_str().unwrap_or_default().to_string();
                Err(Failure::Unreadable { host, what: "neither a code nor an error" })
            }
        }
    }
}
