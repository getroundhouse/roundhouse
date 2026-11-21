use std::path::{Path, PathBuf};

/// One revision of a model's files, from a repository on Hugging Face, pinned so what's downloaded is what was
/// measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelFiles {
    /// The repository on Hugging Face its files are in.
    pub repository: &'static str,
    /// The revision of it they're from, which names the folder they're kept in, so another revision's go beside them.
    pub revision: &'static str,
    pub files: &'static [ModelFile],
}

/// One of a model's files, with its size and SHA-256, so it's checked before it's used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelFile {
    /// Where it is, both in the repository and in the folder the engine loads from.
    pub path: &'static str,
    pub size_bytes: u64,
    pub sha256: [u8; 32],
}

impl ModelFile {
    /// A file, with its SHA-256 in lowercase hex, decoded while compiling, so a build given a malformed one fails
    /// rather than the app.
    pub const fn new(path: &'static str, size_bytes: u64, sha256: &str) -> Self {
        let hex = sha256.as_bytes();
        assert!(hex.len() == 64, "a SHA-256 is 64 hex digits");
        let (mut sha256, mut i) = ([0; 32], 0);
        while i < 32 {
            sha256[i] = nibble(hex[2 * i]) << 4 | nibble(hex[2 * i + 1]);
            i += 1;
        }
        Self { path, size_bytes, sha256 }
    }
}

const fn nibble(digit: u8) -> u8 {
    match digit {
        b'0'..=b'9' => digit - b'0',
        b'a'..=b'f' => digit - b'a' + 10,
        _ => panic!("a SHA-256 is lowercase hex"),
    }
}

impl ModelFiles {
    /// Where its files are kept, under `model_dir`.
    pub fn dir(&self, model_dir: &Path) -> PathBuf {
        model_dir.join(self.revision)
    }

    /// Whether every file is in its folder under `model_dir`. A file is put there only once it's checked, so only its
    /// size is looked at.
    pub fn is_here(&self, model_dir: &Path) -> bool {
        self.files.iter().all(|file| self.has(model_dir, file))
    }

    pub(crate) fn has(&self, model_dir: &Path, file: &ModelFile) -> bool {
        std::fs::metadata(self.dir(model_dir).join(file.path)).is_ok_and(|found| found.len() == file.size_bytes)
    }

    /// How much there is to download.
    pub const fn size_bytes(&self) -> u64 {
        let (mut total, mut i) = (0, 0);
        while i < self.files.len() {
            total += self.files[i].size_bytes;
            i += 1;
        }
        total
    }

    /// Where Hugging Face serves this revision's files, each at its path under it.
    pub fn hugging_face(&self) -> String {
        format!("https://huggingface.co/{}/resolve/{}", self.repository, self.revision)
    }
}

/// How many bytes of a model are in `model_dir`: every revision's files, whole or partly downloaded.
pub fn bytes_on_disk(model_dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(model_dir) else { return 0 };
    let size = |entry: std::fs::DirEntry| match entry.file_type() {
        Ok(kind) if kind.is_dir() => bytes_on_disk(&entry.path()),
        Ok(_) => entry.metadata().map_or(0, |found| found.len()),
        Err(_) => 0,
    };
    entries.flatten().map(size).sum()
}

#[cfg(test)]
mod tests {
    use super::{ModelFile, ModelFiles, bytes_on_disk};

    #[test]
    fn a_sha256_decodes_from_hex() {
        let file = ModelFile::new("a", 1, "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f");
        let counting: [u8; 32] = std::array::from_fn(|i| u8::try_from(i).unwrap());
        assert_eq!(file.sha256, counting);
        assert_eq!(ModelFile::new("a", 1, &"f".repeat(64)).sha256, [255; 32]);
    }

    #[test]
    fn whats_here_is_every_file_at_its_size_and_whats_on_disk_counts_every_revision() {
        const FILES: &[ModelFile] = &[ModelFile { path: "a/b.bin", size_bytes: 3, sha256: [0; 32] }];
        let model = ModelFiles { repository: "test", revision: "one", files: FILES };
        let model_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(model_dir.path().join("one/a")).unwrap();
        std::fs::write(model_dir.path().join("one/a/b.bin"), b"12").unwrap();
        std::fs::create_dir_all(model_dir.path().join("two")).unwrap();
        std::fs::write(model_dir.path().join("two/c.bin.part"), b"12345").unwrap();

        assert!(!model.is_here(model_dir.path()), "a file of the wrong size isn't");
        std::fs::write(model_dir.path().join("one/a/b.bin"), b"123").unwrap();
        assert!(model.is_here(model_dir.path()));
        assert_eq!(bytes_on_disk(model_dir.path()), 8);
        assert_eq!(model.size_bytes(), 3);
    }
}
