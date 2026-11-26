protocol::tokens!(
    /// What an MCP server takes a call with, until it ends.
    AccessToken,
    /// What gets a new access token without the person. An authorization server may take each only once, so only the
    /// hub uses it.
    RefreshToken,
    /// What an authorization server sends back once the person approves, which is good for tokens once, with the
    /// verifier.
    AuthorizationCode,
    /// What only Roundhouse knows of a sign-in, whose hash the authorization server is sent with the page, so a code is
    /// good only for who asked for it, as RFC 7636 has it.
    Verifier,
    /// What the authorization server sends back with the code, so a code is taken only for the sign-in that asked.
    SignInState,
    /// The address of the page the person approves on, which has the sign-in's state in it.
    AuthorizationPage,
);
