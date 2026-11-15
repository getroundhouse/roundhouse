//! The files the copy's code reads by a path from itself, which the copy has to keep for that code to build.

use std::fmt;
use std::fs;
use std::path::Path;

use crate::error::{At, Error};
use crate::files::{CopyPath, files_under, lexical};

/// A file the copy's code reads that the copy doesn't have.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Read {
    /// The file read, from the root of the copy, or as the code names it when that's outside the copy.
    pub file: String,
    pub by: CopyPath,
}

impl fmt::Display for Read {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (read by {})", self.file, self.by)
    }
}

/// Each file the Rust code in `dir` reads by a path from itself, with `include_str!`, `include_bytes!` or `#[path]`,
/// that isn't in `dir`.
pub(crate) fn unkept(dir: &Path) -> Result<Vec<Read>, Error> {
    let mut unread = Vec::new();
    for path in files_under(dir)? {
        let code = path.under(dir);
        let rust = code.extension().is_some_and(|extension| extension == "rs");
        if !rust || fs::symlink_metadata(&code).at(&code)?.is_symlink() {
            continue;
        }
        let text = String::from_utf8_lossy(&fs::read(&code).at(&code)?).into_owned();
        let folder = code.parent().unwrap_or(dir);
        for read in reads(&text) {
            let target = lexical(&folder.join(read));
            // Each as it is without `..`, so `dir` can be named through a folder above it.
            match target.strip_prefix(lexical(dir)) {
                Ok(_) if target.exists() => {}
                Ok(inside) => unread.push(Read { file: inside.to_string_lossy().replace('\\', "/"), by: path.clone() }),
                Err(_) => unread.push(Read { file: read.to_string(), by: path.clone() }),
            }
        }
    }
    Ok(unread)
}

/// The paths `code` reads from, as it writes them.
fn reads(code: &str) -> Vec<&str> {
    let mut found = Vec::new();
    for (marker, assigned) in [("include_str!(", false), ("include_bytes!(", false), ("#[path", true)] {
        for (at, _) in code.match_indices(marker) {
            let rest = code[at + marker.len()..].trim_start();
            let rest = if assigned { rest.strip_prefix('=').map(str::trim_start) } else { Some(rest) };
            if let Some(quoted) = rest.and_then(|rest| rest.strip_prefix('"'))
                && let Some(end) = quoted.find('"')
            {
                found.push(&quoted[..end]);
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_way_of_reading_a_file_is_found() {
        let code = "const A: &str = include_str!(\"../a.md\");\nconst B: &[u8] = include_bytes!(\n    \"b.ttf\",\n);\n\
                    #[path = \"c.rs\"]\nmod c;\nconst D: &str = include_str!(concat!(\"d\", \".md\"));\n";

        assert_eq!(reads(code), ["../a.md", "b.ttf", "c.rs"]);
    }
}
