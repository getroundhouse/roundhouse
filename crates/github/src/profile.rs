/// What GitHub's profile of a user says that its commit identity is made from, as `GET /user` gives it: the name the
/// user goes by and the email it shows everyone, either of which GitHub gives as none, as for a user who hasn't set a
/// name or keeps every email private.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GithubProfile {
    pub name: Option<String>,
    pub email: Option<String>,
}
