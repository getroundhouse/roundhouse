//! What the stand-in answers about apps: making one from its manifest's code, its bot, and what it's asked as the app,
//! with a JWT it checks against the public half of [`MADE_UP_APP_KEY`], as GitHub checks one against the app's, and a
//! user by number, asked with no login, as an app's bot or any account whose own login can't be used is.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jiff::Timestamp;
use ring::signature::{RSA_PKCS1_2048_8192_SHA256, UnparsedPublicKey};
use serde::de::DeserializeOwned;

use super::super::apps::{Claims, GithubAppKey};
use super::script::Answering;
use super::{Answer, MADE_UP_APP_KEY};

/// The most GitHub takes between when a JWT says it was made and when it ends.
const LONGEST_JWT_S: i64 = 10 * 60;

/// What the script has for `head` when it's a call about an app: making one, or asked as one, which a JWT that isn't
/// [`signed_by_the_key`] is refused, as GitHub refuses one, with a 401. `None` for any other call.
pub(super) fn as_the_app(answering: &mut Answering, head: &str) -> Option<Answer> {
    let line = head.lines().next().unwrap_or_default();
    if let Some(code) = line.strip_prefix("POST /app-manifests/").and_then(|rest| rest.split_once("/conversions ")) {
        return Some(answering.conversions.remove(code.0).unwrap_or_else(|| Answer::status(404, Vec::new())));
    }
    if !line.contains(" /app/") {
        return None;
    }
    if !signed_by_the_key(head) {
        return Some(Answer::status(401, Vec::new()));
    }
    if line.starts_with("GET /app/installations?") || line.starts_with("GET /app/installations ") {
        return Some(Answering::next(&mut answering.installations).unwrap_or_else(|| Answer::installations(&[])));
    }
    let installation =
        line.strip_prefix("POST /app/installations/").and_then(|rest| rest.split_once("/access_tokens "));
    let minted = installation.and_then(|(id, _)| answering.minted.get_mut(&id.parse().ok()?));
    Some(minted.and_then(Answering::next).unwrap_or_else(|| Answer::status(404, Vec::new())))
}

/// What the script has for `head` when it's `GET /users/{name}`, such as an app's bot, and `None` for any other call.
pub(super) fn named(answering: &Answering, head: &str) -> Option<Answer> {
    let (name, _) = head.strip_prefix("GET /users/")?.split_once(' ')?;
    let name = name.replace("%5B", "[").replace("%5D", "]");
    Some(answering.named.get(&name).cloned().unwrap_or_else(|| Answer::status(404, Vec::new())))
}

/// What the script has for `head` when it's `GET /user/{number}`, as an account's user is asked for with no login, and
/// `None` for any other call. One asked with a login is a 401.
pub(super) fn numbered(answering: &mut Answering, head: &str) -> Option<Answer> {
    let (number, _) = head.strip_prefix("GET /user/")?.split_once(' ')?;
    if head.lines().any(|line| line.to_ascii_lowercase().starts_with("authorization:")) {
        return Some(Answer::status(401, Vec::new()));
    }
    let answers = answering.numbered.get_mut(&number.parse().ok()?);
    Some(answers.and_then(Answering::next).unwrap_or_else(|| Answer::status(404, Vec::new())))
}

/// Whether `head` asks as an app, with a JWT signed with [`MADE_UP_APP_KEY`] with RS256, from an app by its number,
/// made by now, not ended, and lasting no longer than GitHub takes.
fn signed_by_the_key(head: &str) -> bool {
    let Some(jwt) = head.lines().find_map(|line| line.strip_prefix("authorization: Bearer ")) else { return false };
    let [header, claims, signature] = jwt.split('.').collect::<Vec<_>>()[..] else { return false };
    let key = GithubAppKey::new(MADE_UP_APP_KEY).key_pair().unwrap();
    let public = UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, key.public().as_ref());
    let Ok(signature) = URL_SAFE_NO_PAD.decode(signature) else { return false };
    if public.verify(format!("{header}.{claims}").as_bytes(), &signature).is_err() {
        return false;
    }
    let (Some(header), Some(claims)) = (read::<serde_json::Value>(header), read::<Claims>(claims)) else {
        return false;
    };
    let now = Timestamp::now().as_second();
    header["alg"] == "RS256"
        && claims.iat <= now
        && now < claims.exp
        && claims.exp - claims.iat <= LONGEST_JWT_S
        && claims.iss.parse::<u64>().is_ok()
}

/// What a JWT's `part` says, read as `T`.
fn read<T: DeserializeOwned>(part: &str) -> Option<T> {
    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(part).ok()?).ok()
}
