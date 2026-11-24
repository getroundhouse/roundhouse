use std::path::Path;

/// `path` in single quotes, for a shell.
pub fn quoted(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', r"'\''"))
}
