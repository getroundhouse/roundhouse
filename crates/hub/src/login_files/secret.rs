/// What a login's file holds, which is all it takes to use the login, so a type that's one keeps it out of logs, as
/// `LoginToken`'s `Debug` does by saying only that it's there.
pub(crate) trait LoginSecret: Sized {
    /// The directory its files are in, in the hub's data directory, which is no other kind's, so tidying one kind's
    /// files never removes another's.
    const DIR: &'static str;

    /// What its file holds, which isn't empty, since an empty file holds none.
    fn to_bytes(&self) -> Vec<u8>;

    /// The secret in a file's `bytes`, which are never empty, or `None` when they aren't one.
    fn from_bytes(bytes: Vec<u8>) -> Option<Self>;
}
