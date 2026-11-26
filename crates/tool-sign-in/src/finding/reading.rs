use serde::de::DeserializeOwned;
use tool_server_http::Challenge;
use url::Url;

use super::metadata::{AuthorizationServerMetadata, ServerMetadata};
use super::well_known::{of_authorization_server, of_server, root_of};
use crate::Failure;
use crate::fetch::{Caller, json};

/// What's read of some metadata.
pub(super) enum Read<T> {
    Right(T),
    /// It's there, and says it's of this rather than of what was asked after.
    OfAnother(String),
    /// There's none.
    Nothing,
    /// There may be some, and it couldn't be read.
    Unread(Failure),
}

/// The server's metadata, from where its 401 says, or else from the well-known places.
pub(super) async fn server_metadata(
    calls: &Caller<'_>,
    server: &Url,
    challenge: &Challenge,
) -> Result<Read<ServerMetadata>, Failure> {
    let root = root_of(server);
    let said = challenge.resource_metadata.as_ref().map(|said| calls.address(said)).transpose()?;
    // What's at the host's own well-known place may be of the host, as RFC 9728, section 3.3, has it.
    let roots = if said.is_none() { of_server(&root) } else { Vec::new() };
    let places = said.map_or_else(|| of_server(server), |said| vec![said]);
    let is_right = |place: &Url, metadata: &ServerMetadata| {
        metadata.is_of(server) || (roots.contains(place) && metadata.is_of(&root))
    };
    Ok(first_right(calls, places, is_right, |metadata| metadata.resource).await)
}

/// The metadata of the authorization server called `issuer`.
pub(super) async fn authorization_server_metadata(
    calls: &Caller<'_>,
    issuer: &str,
) -> Result<Read<AuthorizationServerMetadata>, Failure> {
    let places = of_authorization_server(&calls.address(issuer)?);
    let is_right = |_: &Url, metadata: &AuthorizationServerMetadata| metadata.issuer == issuer;
    Ok(first_right(calls, places, is_right, |metadata| metadata.issuer).await)
}

/// The first of what's at `places` that's right. Where none is, whose the first is, by `of`, or else why the first that
/// couldn't be read wasn't, since what's there may be right.
async fn first_right<T: DeserializeOwned>(
    calls: &Caller<'_>,
    places: Vec<Url>,
    is_right: impl Fn(&Url, &T) -> bool,
    of: impl Fn(T) -> String,
) -> Read<T> {
    let (mut another, mut unread) = (None, None);
    for place in places {
        let answer = match calls.get(&place).await {
            Ok(answer) if answer.refused() => continue,
            Ok(answer) if answer.succeeded() => Ok(answer),
            Ok(answer) => Err(answer.failure()),
            Err(failure) => Err(failure),
        };
        match answer.map(|answer| json(&answer)) {
            Ok(Some(metadata)) if is_right(&place, &metadata) => return Read::Right(metadata),
            Ok(Some(metadata)) => {
                another.get_or_insert(of(metadata));
            }
            // A site may answer every path, with a page where there's no metadata.
            Ok(None) => {}
            Err(failure) => {
                unread.get_or_insert(failure);
            }
        }
    }
    match (another, unread) {
        (Some(another), _) => Read::OfAnother(another),
        (None, Some(failure)) => Read::Unread(failure),
        (None, None) => Read::Nothing,
    }
}
