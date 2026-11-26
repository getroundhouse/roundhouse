use std::io;

/// What a login's file holds, which is all it takes to use the login, so a type that's one keeps it out of logs, as
/// `LoginToken`'s `Debug` does by saying only that it's there.
pub(crate) trait LoginSecret: Sized {
    /// The directory its files are in, in the hub's data directory, which is no other kind's, so tidying one kind's
    /// files never removes another's.
    const DIR: &'static str;

    /// What its file holds, which isn't empty, since an empty file holds none. Fails only for a form that can't be
    /// written, such as one serde refuses.
    fn to_bytes(&self) -> io::Result<Vec<u8>>;

    /// What a file's `bytes` hold, which are never empty. Fails for a file that's whole and says it's of a form this
    /// build knows, and that this build can't read all the same, which is kept, as one that can't be read is.
    fn from_bytes(bytes: Vec<u8>) -> io::Result<Stored<Self>>;
}

/// What a login's file holds, as this build reads it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Stored<S> {
    Secret(S),
    /// A secret in a form only a newer build reads, as a hub rolled back to this build finds one, so it's kept for
    /// that build, and this one doesn't use it. Only a kind whose form says which it is can tell one, as a sign-in
    /// with GitHub's does.
    Newer,
    /// None: the file is gone, empty, as one a hub stopped part way through writing can be, or holds what isn't a
    /// secret of the kind, so it's let go of as the hub starts.
    Nothing,
}

impl<S> Stored<S> {
    /// The secret, if this build can use it.
    pub(crate) fn secret(self) -> Option<S> {
        match self {
            Stored::Secret(secret) => Some(secret),
            Stored::Newer | Stored::Nothing => None,
        }
    }
}
