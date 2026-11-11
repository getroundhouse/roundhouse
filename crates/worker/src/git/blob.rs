//! A file as a commit has it, read from the repository rather than the worktree, so a client can show what a change
//! replaced: where it is, how big it is, and its bytes.

use std::path::Path;
use std::process::Stdio;

use protocol::client::{ApiError, ErrorCode};
use protocol::model::CommitId;
use tokio::io::AsyncReadExt as _;

use super::{command, git_error, run};

/// A file in a commit, by the object that holds it.
#[derive(Debug)]
pub(crate) struct Blob {
    object: String,
    pub(crate) bytes: u64,
}

/// The file at `path` in `commit`, from the repository of the worktree at `worktree`. Refused as an invalid request
/// unless `commit` is a full object ID, so it can't be taken for an option or name a branch that has moved since the
/// diff, and when the commit has a folder or a submodule there. Refused as not found when the repository has no such
/// commit, or the commit has nothing there. A link isn't followed, and is the path it leads to, as the commit holds it.
pub(crate) async fn find(worktree: &Path, commit: &CommitId, path: &str) -> Result<Blob, ApiError> {
    if !is_object_id(commit.as_str()) {
        return Err(ApiError::new(ErrorCode::InvalidRequest, format!("{commit} isn't a commit's full object ID")));
    }
    // Literal, so a name such as `:notes.txt` is only itself, rather than what git reads into it.
    let args = ["--literal-pathspecs", "ls-tree", "-l", "-z", commit.as_str(), "--", path];
    let output = run(worktree, &args).await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ApiError::new(ErrorCode::NotFound, format!("{commit} isn't here: {}", stderr.trim())));
    }
    let entry = output.stdout.split(|byte| *byte == 0).filter_map(entry).find(|entry| entry.path == path.as_bytes());
    let Some(entry) = entry else {
        return Err(ApiError::new(ErrorCode::NotFound, format!("{path} isn't in {commit}")));
    };
    let (b"blob", Some(bytes)) = (entry.kind, entry.bytes) else {
        return Err(ApiError::new(ErrorCode::InvalidRequest, format!("{path} isn't a file in {commit}")));
    };
    Ok(Blob { object: entry.object, bytes })
}

impl Blob {
    /// Its bytes, all of them.
    pub(crate) async fn read(&self, worktree: &Path) -> Result<Vec<u8>, ApiError> {
        let output = run(worktree, &["cat-file", "blob", &self.object]).await?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(git_error(format!("couldn't read {}: {}", self.object, stderr.trim())));
        }
        Ok(output.stdout)
    }

    /// Its first `most` bytes, or all of it if it's shorter, and no more, so a big file costs only its start. Git is
    /// stopped once they're read, if it hasn't finished writing the rest.
    pub(crate) async fn start(&self, worktree: &Path, most: usize) -> Result<Vec<u8>, ApiError> {
        let failed = |e: std::io::Error| git_error(format!("couldn't read {}: {e}", self.object));
        let mut git = command(worktree, &["cat-file", "blob", &self.object])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(failed)?;
        let Some(mut stdout) = git.stdout.take() else {
            return Err(git_error(format!("couldn't read {}: git's output wasn't piped", self.object)));
        };
        let mut start = vec![0; most.min(usize::try_from(self.bytes).unwrap_or(usize::MAX))];
        let read = stdout.read_exact(&mut start).await;
        // Closed first, so git can't be left waiting to write before it says why it stopped.
        drop(stdout);
        if let Err(e) = read {
            let status = git.wait().await.map_err(failed)?;
            return Err(git_error(format!("couldn't read {}: {e}, and git ended with {status}", self.object)));
        }
        Ok(start)
    }
}

/// What git names a commit by in full: 40 hex digits, or 64 in a repository that uses SHA-256.
fn is_object_id(id: &str) -> bool {
    matches!(id.len(), 40 | 64) && id.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// One entry of `ls-tree -l`.
struct Entry<'a> {
    kind: &'a [u8],
    object: String,
    /// `None` for a folder or a submodule, which have no size.
    bytes: Option<u64>,
    path: &'a [u8],
}

/// An entry of `ls-tree -l -z`, which is its mode, its kind, its object and its size, with spaces between, then a tab
/// and its path. A folder's size, or a submodule's, is `-`.
fn entry(line: &[u8]) -> Option<Entry<'_>> {
    let tab = line.iter().position(|byte| *byte == b'\t')?;
    let (about, path) = (std::str::from_utf8(&line[..tab]).ok()?, &line[tab + 1..]);
    let mut fields = about.split_ascii_whitespace();
    let (_mode, kind, object, bytes) = (fields.next()?, fields.next()?, fields.next()?, fields.next()?);
    let bytes = if bytes == "-" { None } else { Some(bytes.parse().ok()?) };
    Some(Entry { kind: kind.as_bytes(), object: object.to_string(), bytes, path })
}

#[cfg(test)]
mod tests {
    use protocol::model::CommitId;

    use super::{entry, find, is_object_id};
    use crate::git::tests::{new_repo, run};

    /// More than a pipe holds, so git is still writing when the start has been read, and is stopped.
    #[tokio::test]
    async fn the_start_of_a_file_is_as_much_as_asked_for_and_no_more() {
        let dir = tempfile::tempdir().unwrap();
        let repo = new_repo(dir.path());
        std::fs::write(repo.join("long.txt"), "a".repeat(1024 * 1024)).unwrap();
        run(&repo, &["add", "."]);
        run(&repo, &["commit", "-qm", "long"]);
        let commit = CommitId::from(run(&repo, &["rev-parse", "HEAD"]));
        let (long, short) =
            (find(&repo, &commit, "long.txt").await.unwrap(), find(&repo, &commit, "a.txt").await.unwrap());

        let (start, all) = (long.start(&repo, 10).await.unwrap(), short.start(&repo, 10).await.unwrap());

        assert_eq!(start, b"aaaaaaaaaa");
        assert_eq!(all, b"one\n", "all of one that's shorter");
    }

    #[test]
    fn only_a_full_object_id_names_a_commit() {
        assert!(is_object_id(&"a1".repeat(20)));
        assert!(is_object_id(&"0f".repeat(32)));

        for id in ["HEAD", "main", "3f4c19c", "--output=x", &"A1".repeat(20), &"g1".repeat(20), &"a1".repeat(21)] {
            assert!(!is_object_id(id), "{id}");
        }
    }

    #[test]
    fn an_entry_is_its_kind_its_object_its_size_and_its_path() {
        let line = b"100644 blob 5d6e7f8a      2048\tdocs/a shot.png";

        let read = entry(line).unwrap();

        assert_eq!((read.kind, read.object.as_str(), read.bytes), (&b"blob"[..], "5d6e7f8a", Some(2048)));
        assert_eq!(read.path, b"docs/a shot.png");
        assert_eq!(entry(b"160000 commit 5d6e7f8a       -\tvendor/inner").unwrap().bytes, None, "a submodule");
    }
}
