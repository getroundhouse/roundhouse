/// The name of the file a login's secret is in, which the hub makes as the login is connected and keeps on its row: a
/// UUID, lowercase and with its hyphens.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct LoginFile(String);

impl LoginFile {
    pub(crate) fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// `None` for anything but a name the hub makes, so nothing read back can climb out of the directory.
    pub(crate) fn parse(name: &str) -> Option<Self> {
        let uuid = uuid::Uuid::try_parse(name).ok()?;
        (uuid.hyphenated().to_string() == name).then(|| Self(name.to_string()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}
