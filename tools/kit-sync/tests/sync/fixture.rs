//! A repository standing in for upstream's, whose releases are tags, and one standing in for this one, whose copy is
//! committed as the real one is.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use kit_sync::{
    COPY, Commit, Error, Git, Kept, NOTICE, PATCHES, PatchName, Patched, Pin, RECORD, Record, Release, Synced,
};
use tempfile::TempDir;

/// The crates the copy keeps.
pub(crate) const CRATES: [&str; 5] =
    ["crates/kit", "crates/base", "crates/component", "crates/component-macros", "crates/assets"];
/// The files the copy keeps beside them.
pub(crate) const FILES: [&str; 4] = [".rustfmt.toml", "LICENSE-APACHE", "README.md", "themes/aurora.json"];
/// What the copy leaves out of a crate it keeps.
pub(crate) const LEFT_OUT: &str = "crates/base/examples";
/// What upstream has that the copy doesn't keep: a demo app, an example and the website.
pub(crate) const UNKEPT: [&str; 3] = ["crates/story/Cargo.toml", "examples/hello/Cargo.toml", "website/index.md"];
/// Upstream's link from a crate to the license at the top, which the real copy has too.
pub(crate) const LINK: &str = "crates/component-macros/LICENSE-APACHE";

/// What the copy keeps of upstream's in these tests, as the real one does.
pub(crate) fn kept() -> Vec<Kept> {
    let whole = CRATES.into_iter().chain(["Cargo.toml"]).chain(FILES);
    whole.map(Kept::from).chain([Kept::from(format!("!{LEFT_OUT}").as_str())]).collect()
}

/// A file of numbered lines, some `changed`, so changes far enough apart merge.
pub(crate) fn lines(count: usize, changed: &[(usize, &str)]) -> String {
    (1..=count)
        .map(|number| match changed.iter().find(|(line, _)| *line == number) {
            Some((_, text)) => format!("{text}\n"),
            None => format!("line {number}\n"),
        })
        .collect()
}

/// `body` preceded by the Rust comment notice line the copy puts on changed files.
pub(crate) fn noticed_rs(body: &str) -> String {
    format!("// {NOTICE}\n{body}")
}

/// Upstream's workspace manifest, on `gpui`, with `after_members` on the line that ends its members.
pub(crate) fn workspace(gpui: &str, after_members: &str) -> String {
    let members = CRATES.into_iter().chain(["crates/base/examples/native", "crates/story", "examples/hello"]);
    let listed: String = members.flat_map(|member| ["    \"", member, "\",\n"]).collect();
    format!(
        "[workspace]\ndefault-members = [\"crates/story\"]\nmembers = [\n{listed}]{after_members}\nresolver = \"2\"\n\n\
         [workspace.package]\nedition = \"2024\"\n\n\
         [workspace.dependencies]\ngpui = {{ package = \"gpui-pre\", version = \"{gpui}\" }}\n"
    )
}

/// Runs git in `dir` for a fixture, without the person's settings, which could change what it makes.
pub(crate) fn git(dir: &Path, args: &[&str]) -> String {
    let nothing = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let output = Command::new("git")
        .current_dir(dir)
        .env_clear()
        .envs(std::env::vars_os().filter(|(name, _)| !name.to_string_lossy().starts_with("GIT_")))
        .env("GIT_CONFIG_GLOBAL", nothing)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args(["-c", &format!("core.attributesFile={nothing}"), "-c", &format!("core.excludesFile={nothing}")])
        .args(["-c", "user.name=test", "-c", "user.email=test@localhost"])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

/// Writes each of `files`, as (its path from `dir`, what it holds), in place of what's there.
pub(crate) fn write(dir: &Path, files: &[(&str, &str)]) {
    for (name, text) in files {
        let path = dir.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        if fs::symlink_metadata(&path).is_ok() {
            fs::remove_file(&path).unwrap();
        }
        fs::write(path, text).unwrap();
    }
}

fn release(upstream: &Path, tag: &str) -> Commit {
    git(upstream, &["add", "-A"]);
    git(upstream, &["commit", "-q", "-m", tag]);
    tag_head(upstream, tag)
}

fn tag_head(upstream: &Path, tag: &str) -> Commit {
    git(upstream, &["tag", "-a", "-m", tag, tag]);
    Commit::new(git(upstream, &["rev-parse", "HEAD"]).trim())
}

#[cfg(unix)]
fn link(target: &str, at: &Path) {
    std::os::unix::fs::symlink(target, at).unwrap();
}

#[cfg(windows)]
fn link(target: &str, at: &Path) {
    std::os::windows::fs::symlink_file(target, at).unwrap();
}

/// Upstream's repository, and this repository with no copy yet, with upstream's first release, `v1`, tagged.
pub(crate) struct Stand {
    _work: TempDir,
    pub(crate) work: PathBuf,
    pub(crate) upstream: PathBuf,
    pub(crate) root: PathBuf,
    pub(crate) copy: PathBuf,
    pub(crate) patches: PathBuf,
    pub(crate) first: Commit,
}

impl Stand {
    pub(crate) fn new() -> Self {
        let held = tempfile::tempdir().unwrap();
        let work = held.path().canonicalize().unwrap();
        let upstream = work.join("upstream");
        fs::create_dir(&upstream).unwrap();
        git(&upstream, &["init", "-q"]);
        // As GitHub does, so a commit can be fetched by its hash.
        git(&upstream, &["config", "uploadpack.allowAnySHA1InWant", "true"]);
        for crate_folder in CRATES {
            let name = if crate_folder == "crates/kit" { "gpui-kit" } else { crate_folder };
            let manifest = format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n");
            write(&upstream, &[(&format!("{crate_folder}/Cargo.toml"), &manifest)]);
            write(&upstream, &[(&format!("{crate_folder}/src/lib.rs"), &lines(12, &[]))]);
        }
        for path in FILES.into_iter().chain(UNKEPT) {
            write(&upstream, &[(path, "upstream's\n")]);
        }
        write(
            &upstream,
            &[
                ("Cargo.toml", &workspace("=0.3.11", "")),
                ("crates/base/examples/native/Cargo.toml", "[package]\nname = \"native\"\n"),
                // Left out of the copy, so what it reads, which upstream doesn't have here, doesn't matter.
                (
                    "crates/base/examples/showcase/mod.rs",
                    "const FONT: &[u8] = include_bytes!(\"../../../web/a.ttf\");\n",
                ),
                ("crates/component/src/old.rs", "old\n"),
                // Read from elsewhere in upstream's repository, as the real crates read their fixtures.
                (
                    "crates/component/src/theme.rs",
                    "const AURORA: &str = include_str!(\"../../../themes/aurora.json\");\n",
                ),
                ("LICENSE-APACHE", "Apache License\n"),
            ],
        );
        link("../../LICENSE-APACHE", &upstream.join(LINK));
        let first = release(&upstream, "v1");
        // Named through the folder beside it, as `main` names the repository through `tools/kit-sync/../..`.
        let root = work.join("upstream/../root");
        fs::create_dir(&root).unwrap();
        git(&root, &["init", "-q"]);
        write(
            &root,
            &[
                (".gitignore", "target/\ndist/\n"),
                (
                    "Cargo.toml",
                    "[workspace.dependencies]\ngpui-kit = { path = \"third_party/gpui-kit/vendor/crates/kit\", \
                 version = \"=0.0.1\", features = [\"syntect\"] }\ngpui-pre-apple = { version = \"=0.3.10\" }\n",
                ),
            ],
        );
        let stand =
            Self { copy: root.join(COPY), patches: root.join(PATCHES), _work: held, work, upstream, root, first };
        stand.commit_root();
        stand
    }

    /// Commits what upstream has as a release, tagged `tag`.
    pub(crate) fn release(&self, tag: &str) -> Commit {
        release(&self.upstream, tag)
    }

    /// Tags upstream's commit that's checked out, and returns it.
    pub(crate) fn tag(&self, tag: &str) -> Commit {
        tag_head(&self.upstream, tag)
    }

    pub(crate) fn pin(&self, release: &str, commit: &Commit, kept: Vec<Kept>) -> Pin {
        let upstream = format!("file://{}", self.upstream.display());
        Pin { upstream, release: Release::new(release), commit: commit.clone(), kept }
    }

    /// Syncs the copy to `release`, keeping what the real copy does, with the person's git as this process has it.
    pub(crate) fn sync(&self, release: &str, commit: &Commit) -> Result<Synced, Error> {
        self.sync_keeping(release, commit, kept())
    }

    pub(crate) fn sync_keeping(&self, release: &str, commit: &Commit, kept: Vec<Kept>) -> Result<Synced, Error> {
        kit_sync::sync(&Git::new(std::env::vars_os()), &self.root, &self.pin(release, commit, kept))
    }

    /// Syncs the copy to `release` with git run in `environment` on top of this process's.
    pub(crate) fn sync_in(
        &self,
        environment: &[(OsString, OsString)],
        release: &str,
        commit: &Commit,
    ) -> Result<Synced, Error> {
        let git = Git::new(std::env::vars_os().chain(environment.iter().cloned()));
        kit_sync::sync(&git, &self.root, &self.pin(release, commit, kept()))
    }

    /// Records changes in `copy` into `<name>.patch`.
    pub(crate) fn patch(&self, name: &str) -> Result<Patched, Error> {
        let upstream = format!("file://{}", self.upstream.display());
        let name = PatchName::parse(name)?;
        kit_sync::patch(&Git::new(std::env::vars_os()), &self.root, &upstream, &name)
    }

    /// Records changes in `copy` into `<name>.patch` with git run in `environment`.
    pub(crate) fn patch_in(&self, environment: &[(OsString, OsString)], name: &str) -> Result<Patched, Error> {
        let git = Git::new(std::env::vars_os().chain(environment.iter().cloned()));
        let upstream = format!("file://{}", self.upstream.display());
        let name = PatchName::parse(name)?;
        kit_sync::patch(&git, &self.root, &upstream, &name)
    }

    /// Drops `<name>.patch` from the copy and the record.
    pub(crate) fn drop_patch(&self, name: &str) -> Result<(), Error> {
        let name = PatchName::parse(name)?;
        kit_sync::drop_patch(&Git::new(std::env::vars_os()), &self.root, &name)
    }

    /// Runs the offline policy check against `release` and `commit`.
    pub(crate) fn verify(&self, release: &str, commit: &Commit) -> Result<(), Error> {
        kit_sync::verify(&Git::new(std::env::vars_os()), &self.root, &self.pin(release, commit, kept()))
    }

    pub(crate) fn commit_root(&self) {
        git(&self.root, &["add", "-A"]);
        git(&self.root, &["commit", "-q", "--allow-empty", "-m", "Root"]);
    }

    pub(crate) fn read(&self, path: &str) -> String {
        fs::read_to_string(self.copy.join(path)).unwrap()
    }

    pub(crate) fn read_patch(&self, name: &str) -> String {
        fs::read_to_string(self.patches.join(format!("{name}.patch"))).unwrap()
    }

    pub(crate) fn made(&self) -> Record {
        assert!(self.root.join(RECORD).exists());
        Record::read(&self.root).unwrap().unwrap()
    }

    /// The versions of gpui-kit and gpui-pre-apple in this repository's `Cargo.toml`.
    pub(crate) fn versions(&self) -> (String, String) {
        let manifest: toml::Table = fs::read_to_string(self.root.join("Cargo.toml")).unwrap().parse().unwrap();
        let pinned = &manifest["workspace"]["dependencies"];
        let version = |name: &str| pinned[name]["version"].as_str().unwrap().to_string();
        (version("gpui-kit"), version("gpui-pre-apple"))
    }
}
