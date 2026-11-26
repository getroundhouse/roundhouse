use protocol::model::ToolScopes;

/// The scope that asks for a refresh token, as OIDC names it.
const OFFLINE_ACCESS: &str = "offline_access";

/// `scopes`, and the one that asks for a refresh token. Empty stays empty, since asking for that one alone would ask for
/// nothing of what the authorization server gives when it's left to it. Where `scopes` are as many as are kept, it goes
/// first, in place of the last of them, since a sign-in that can't be renewed costs more than one scope does.
#[must_use]
pub(crate) fn offline(scopes: ToolScopes) -> ToolScopes {
    if scopes.is_empty() {
        return scopes;
    }
    let refresh = ToolScopes::of([OFFLINE_ACCESS]);
    let in_front = refresh.clone().and(&scopes);
    let asked = scopes.and(&refresh);
    if asked.has(OFFLINE_ACCESS) { asked } else { in_front }
}

/// `scopes`, but for the one that asks for a refresh token.
#[must_use]
pub(crate) fn without_offline(scopes: ToolScopes) -> ToolScopes {
    scopes.without(&ToolScopes::of([OFFLINE_ACCESS]))
}

pub(crate) fn has_offline(scopes: &ToolScopes) -> bool {
    scopes.has(OFFLINE_ACCESS)
}

/// Whether an authorization server that says it supports `scopes` has the one that asks for a refresh token among
/// them, however many it names, of which a [`ToolScopes`] keeps only some.
pub(crate) fn lists_offline(scopes: &[String]) -> bool {
    scopes.iter().any(|scope| scope == OFFLINE_ACCESS)
}

#[cfg(test)]
mod tests {
    use protocol::model::ToolScopes;

    use super::*;

    /// A sign-in that can't be renewed costs more than the last of a server's scopes.
    #[test]
    fn the_refresh_tokens_scope_is_asked_for_even_where_there_are_as_many_as_are_kept() {
        let of = |numbers: std::ops::Range<usize>| ToolScopes::of(numbers.map(|n| format!("scope:{n}")));

        let few = offline(of(0..3));
        let most = offline(of(0..32));

        assert_eq!(few.to_string(), "scope:0 scope:1 scope:2 offline_access");
        assert_eq!(most, ToolScopes::of([OFFLINE_ACCESS]).and(&of(0..31)));
        assert_eq!(offline(ToolScopes::default()), ToolScopes::default());
    }
}
