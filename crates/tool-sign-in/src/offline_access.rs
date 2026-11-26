use protocol::model::ToolScopes;

/// The scope that asks for a refresh token, as OIDC names it.
const OFFLINE_ACCESS: &str = "offline_access";

/// `scopes`, and the one that asks for a refresh token. Empty stays empty, since asking for that one alone would ask for
/// nothing of what the authorization server gives when it's left to it.
#[must_use]
pub(crate) fn offline(scopes: ToolScopes) -> ToolScopes {
    if scopes.is_empty() { scopes } else { scopes.and(&ToolScopes::of([OFFLINE_ACCESS])) }
}

/// `scopes`, but for the one that asks for a refresh token.
#[must_use]
pub(crate) fn without_offline(scopes: ToolScopes) -> ToolScopes {
    scopes.without(&ToolScopes::of([OFFLINE_ACCESS]))
}

pub(crate) fn has_offline(scopes: &ToolScopes) -> bool {
    scopes.has(OFFLINE_ACCESS)
}
