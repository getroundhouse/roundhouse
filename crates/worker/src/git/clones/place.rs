use std::path::PathBuf;

use protocol::worker::RepoSource;
use uuid::Uuid;

/// The folder in the clones directory where each clone is made, beside the lock workers take turns at making it by.
/// Its name starts with a dot, which no clone's does, so what's listed in the clones directory is the whole clones.
pub(super) const MAKING: &str = ".making";

/// The directory where the workers on a machine keep their clones, which their provider names.
pub(crate) struct Clones {
    dir: PathBuf,
}

impl Clones {
    pub(crate) fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Where git runs for `source`'s repository: the checkout, or where the clone of the address is, whether or not
    /// it's there.
    pub(crate) fn repository(&self, source: &RepoSource) -> PathBuf {
        match source {
            RepoSource::Checkout { path } => PathBuf::from(path),
            RepoSource::Clone { url } => self.of(url),
        }
    }

    /// Where the clone of `url` is, once it's whole.
    pub(super) fn of(&self, url: &str) -> PathBuf {
        self.dir.join(name(url))
    }

    /// Where the clone of `url` is made, and the file of the lock workers take turns at making it by, which is never
    /// deleted, since they share it.
    pub(super) fn making(&self, url: &str) -> (PathBuf, PathBuf) {
        let (making, name) = (self.dir.join(MAKING), name(url));
        (making.join(&name), making.join(format!("{name}.lock")))
    }
}

/// A clone's name: the repository's, as the last part of its address has it, so a person can tell which repository
/// each is, and the address's UUID in the URL namespace, which every build and machine works out the same, so no two
/// addresses share one, however alike they are, such as one with `.git` at the end and one without. Ends with `.git`,
/// as a bare repository's name does.
fn name(url: &str) -> String {
    let last = url.trim_end_matches(['/', '\\']).rsplit(['/', '\\', ':']).next().unwrap_or_default();
    let repository: String = last
        .strip_suffix(".git")
        .unwrap_or(last)
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '-' })
        .collect();
    let repository = match repository.trim_start_matches('.') {
        "" => "repository",
        named => named,
    };
    format!("{repository}-{}.git", Uuid::new_v5(&Uuid::NAMESPACE_URL, url.as_bytes()).simple())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn a_clone_is_named_for_its_repository_and_its_address() {
        let named = |url: &str| name(url).rsplit_once('-').unwrap().0.to_string();

        assert_eq!(named("https://github.com/ada/rails.git"), "rails");
        assert_eq!(named("git@github.com:ada/rails"), "rails");
        assert_eq!(named("https://github.com/ada/rails/"), "rails");
        assert_eq!(named(r"C:\code\my app.git"), "my-app");
        assert_eq!(named("https://example.com/.dotfiles"), "dotfiles");
        assert_eq!(named("https://example.com/"), "example.com");
        assert_eq!(named("/"), "repository");
        assert_ne!(name("https://github.com/ada/rails.git"), name("https://github.com/ada/rails"));
        assert_eq!(name("https://github.com/ada/rails"), name("https://github.com/ada/rails"));
        assert_eq!(Path::new(&name("https://github.com/ada/rails")).extension(), Some("git".as_ref()));
    }

    /// The same on every build and machine, so the workers of any build share one, rather than each making its own:
    /// the UUID Python's `uuid.uuid5(uuid.NAMESPACE_URL, url)` gives too.
    #[test]
    fn an_address_names_the_same_clone_on_every_build() {
        assert_eq!(name("https://github.com/ada/rails.git"), "rails-32c5b9a70a5f50d8b81edaaa133969f3.git");
    }
}
