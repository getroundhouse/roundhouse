use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use protocol::client::{ApiError, ErrorCode};
use protocol::model::GithubAppId;
use ring::rand::SystemRandom;
use ring::signature::{RSA_PKCS1_SHA256, RsaKeyPair};
use serde::{Deserialize, Serialize};

protocol::tokens!(
    /// An app of your own's private key, in PEM, as GitHub gave it once, as the app was made. It signs what the hub
    /// asks GitHub as the app, which is all it takes to act as the app, so it's kept in a login file and sent nowhere.
    GithubAppKey,
    /// What the hub asks GitHub as an app with: a JSON web token its key signed, which lasts [`LASTS`].
    AppJwt,
);

/// How long before the hub's clock a JWT says it was made, so a clock a little ahead of GitHub's makes one GitHub
/// takes, as GitHub's docs give.
const MADE_BEFORE: Duration = Duration::from_secs(60);

/// How long a JWT lasts from when it says it was made: under the 10 minutes GitHub takes at most, so a clock a little
/// behind GitHub's makes one it takes too.
const LASTS: Duration = Duration::from_mins(9);

/// What a JWT says, as GitHub reads it: who it's from, as the app's number, and when it was made and ends, in seconds
/// since the epoch.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub(crate) struct Claims {
    pub(crate) iat: i64,
    pub(crate) exp: i64,
    pub(crate) iss: String,
}

impl GithubAppKey {
    /// A JWT as `app`, signed with this key, made at `now_ms` by the hub's clock, with RS256, as GitHub takes one.
    /// Refused with `Conflict` for a key that isn't an RSA private key in PEM, which GitHub gives no app.
    pub(crate) fn signed_as(&self, app: GithubAppId, now_ms: i64) -> Result<AppJwt, ApiError> {
        let key = self.key_pair().ok_or_else(|| {
            ApiError::new(ErrorCode::Conflict, "the app's key isn't an RSA private key Roundhouse can sign with")
        })?;
        let now = now_ms.div_euclid(1000);
        let claims = Claims { iat: now - seconds(MADE_BEFORE), exp: now + seconds(LASTS), iss: app.to_string() };
        let claims = serde_json::to_vec(&claims).map_err(|e| ApiError::new(ErrorCode::Internal, e.to_string()))?;
        let signing =
            format!("{}.{}", URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256","typ":"JWT"}"#), URL_SAFE_NO_PAD.encode(claims));
        let mut signature = vec![0; key.public().modulus_len()];
        key.sign(&RSA_PKCS1_SHA256, &SystemRandom::new(), signing.as_bytes(), &mut signature)
            .map_err(|_| ApiError::new(ErrorCode::Internal, "couldn't sign as the app with its key"))?;
        Ok(AppJwt::new(format!("{signing}.{}", URL_SAFE_NO_PAD.encode(signature))))
    }

    /// The key, from its PEM: `RSA PRIVATE KEY`, as GitHub gives one, or `PRIVATE KEY`, as PKCS #8 wraps one. `None`
    /// for anything else.
    pub(crate) fn key_pair(&self) -> Option<RsaKeyPair> {
        let pem = self.as_str().trim();
        let (label, rest) = pem.strip_prefix("-----BEGIN ")?.split_once("-----")?;
        let body = rest.trim_end().strip_suffix(&format!("-----END {label}-----"))?;
        let der = STANDARD.decode(body.split_whitespace().collect::<String>()).ok()?;
        match label {
            "RSA PRIVATE KEY" => RsaKeyPair::from_der(&der).ok(),
            "PRIVATE KEY" => RsaKeyPair::from_pkcs8(&der).ok(),
            _ => None,
        }
    }
}

#[expect(clippy::cast_possible_wrap, reason = "a JWT's few minutes are far under i64's seconds")]
const fn seconds(duration: Duration) -> i64 {
    duration.as_secs() as i64
}
