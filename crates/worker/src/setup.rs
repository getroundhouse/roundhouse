//! Getting a new worktree ready: copying local files it needs, and running the repository's scripts.
//!
//! Reads Conductor's settings files, so a repository set up for Conductor works here unchanged.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use protocol::client::{ApiError, ErrorCode};
use tokio::process::Command;
use tracing::warn;

use crate::git;

/// What Conductor copies with nothing configured.
const DEFAULT_INCLUDE: &str = ".env*";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub setup: Option<String>,
    pub archive: Option<String>,
    pub file_include_globs: Option<String>,
}

impl Settings {
    fn parse(text: &str) -> Result<Self, String> {
        let value: toml::Table = text.parse().map_err(|e: toml::de::Error| e.to_string())?;
        let script =
            |name: &str| value.get("scripts").and_then(|s| s.get(name)).and_then(|s| s.as_str()).map(str::to_string);
        Ok(Self {
            setup: script("setup"),
            archive: script("archive"),
            file_include_globs: value.get("file_include_globs").and_then(|v| v.as_str()).map(str::to_string),
        })
    }

    /// Fills in whatever this doesn't set from a less specific file.
    fn or(self, fallback: Self) -> Self {
        Self {
            setup: self.setup.or(fallback.setup),
            archive: self.archive.or(fallback.archive),
            file_include_globs: self.file_include_globs.or(fallback.file_include_globs),
        }
    }
}

/// Most specific first: your local overrides, then what's committed on this branch, then your defaults.
/// A file that doesn't parse is skipped with a warning, because a typo shouldn't stop a workspace.
pub fn load_settings(repo: &Path, worktree: &Path, home: Option<&Path>, warnings: &mut Vec<String>) -> Settings {
    let mut files = vec![repo.join(".conductor/settings.local.toml"), worktree.join(".conductor/settings.toml")];
    files.extend(home.map(|home| home.join(".conductor/settings.toml")));
    let mut settings = Settings::default();
    for file in files {
        let Ok(text) = std::fs::read_to_string(&file) else { continue };
        match Settings::parse(&text) {
            Ok(layer) => settings = settings.or(layer),
            Err(e) => warnings.push(format!("ignoring {}: {e}", file.display())),
        }
    }
    settings
}

fn include_patterns(repo: &Path, settings: &Settings) -> Vec<String> {
    let text = std::fs::read_to_string(repo.join(".worktreeinclude"))
        .ok()
        .or_else(|| settings.file_include_globs.clone())
        .unwrap_or_else(|| DEFAULT_INCLUDE.to_string());
    text.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#')).map(str::to_string).collect()
}

/// Copies gitignored files matching the patterns from the main checkout into the worktree, so they
/// don't have to be recreated by hand. Git does the matching, so patterns mean what they do in
/// `.gitignore`. Only ignored files qualify: anything else is either tracked, and already there, or
/// the user's work in progress. Returns what was copied.
pub async fn copy_local_files(
    repo: &Path,
    worktree: &Path,
    settings: &Settings,
    warnings: &mut Vec<String>,
) -> Vec<String> {
    let patterns = include_patterns(repo, settings);
    if patterns.is_empty() {
        return Vec::new();
    }
    let standard: Vec<String> =
        ["ls-files", "-z", "--others", "--ignored", "--exclude-standard"].map(String::from).to_vec();
    let mut matching: Vec<String> = ["ls-files", "-z", "--others", "--ignored"].map(String::from).to_vec();
    for pattern in &patterns {
        matching.push(format!("--exclude={pattern}"));
    }
    let (ignored, matched) =
        match tokio::try_join!(git::list_nul_separated(repo, &standard), git::list_nul_separated(repo, &matching)) {
            Ok(lists) => lists,
            Err(e) => {
                warnings.push(format!("couldn't list files to copy: {}", e.message));
                return Vec::new();
            }
        };
    let ignored: HashSet<String> = ignored.into_iter().collect();
    let mut copied = Vec::new();
    for relative in matched.into_iter().filter(|file| ignored.contains(file)) {
        let (from, to) = (repo.join(&relative), worktree.join(&relative));
        // A symlink could point anywhere, and a stale copy of its target isn't what anyone asked for.
        if !std::fs::symlink_metadata(&from).is_ok_and(|m| m.is_file()) || to.exists() {
            continue;
        }
        let result =
            to.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::copy(&from, &to).map(drop));
        match result {
            Ok(()) => copied.push(relative),
            Err(e) => warnings.push(format!("couldn't copy {relative}: {e}")),
        }
    }
    copied
}

/// Runs a script in the worktree, with its output going to `log`, and returns its exit code. In its own
/// process group, so stopping it can stop what it started.
pub async fn run_script(
    script: &str,
    worktree: &Path,
    env: &[(String, String)],
    log: &Path,
    on_start: impl FnOnce(u32),
) -> Result<std::process::ExitStatus, ApiError> {
    let failed = |e: std::io::Error| ApiError::new(ErrorCode::Worker, format!("couldn't run the script: {e}"));
    let out = std::fs::OpenOptions::new().create(true).append(true).open(log).map_err(failed)?;
    let err = out.try_clone().map_err(failed)?;
    let mut child = Command::new("bash")
        .args(["-c", script])
        .current_dir(worktree)
        .envs(env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .process_group(0)
        .kill_on_drop(true)
        .spawn()
        .map_err(failed)?;
    if let Some(pid) = child.id() {
        on_start(pid);
    }
    child.wait().await.map_err(failed)
}

pub fn append_log(log: &Path, line: &str) {
    use std::io::Write as _;
    if let Err(e) =
        std::fs::OpenOptions::new().create(true).append(true).open(log).and_then(|mut f| writeln!(f, "{line}"))
    {
        warn!("couldn't write to {}: {e}", log.display());
    }
}

pub fn setup_log_path(state_dir: Option<&Path>, worktree: &Path) -> PathBuf {
    match state_dir {
        Some(dir) => dir.join("setup.log"),
        // Only without an instance directory, in tests. Git ignores nothing here, so it's outside the worktree.
        None => std::env::temp_dir().join(format!("setup-{}.log", worktree.display().to_string().replace('/', "-"))),
    }
}

/// The end of the log, because a long install prints far more than anyone reads.
pub fn read_log_tail(log: &Path, max_bytes: u64) -> String {
    use std::io::{Read as _, Seek as _, SeekFrom};
    let Ok(mut file) = std::fs::File::open(log) else { return String::new() };
    let length = file.metadata().map_or(0, |m| m.len());
    if length > max_bytes {
        let _ = file.seek(SeekFrom::Start(length - max_bytes));
    }
    let mut bytes = Vec::new();
    let _ = file.read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(cwd: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    }

    /// A repository with ignored local files, and a worktree that doesn't have them.
    fn repo_and_worktree(dir: &Path, gitignore: &str) -> (PathBuf, PathBuf) {
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        run(&repo, &["init", "-q", "-b", "main"]);
        std::fs::write(repo.join(".gitignore"), gitignore).unwrap();
        std::fs::write(repo.join("tracked.txt"), "tracked\n").unwrap();
        run(&repo, &["add", "."]);
        run(&repo, &["commit", "-qm", "first"]);
        let worktree = dir.join("worktree");
        run(&repo, &["worktree", "add", "-q", "-b", "w", worktree.to_str().unwrap()]);
        (repo, worktree)
    }

    fn write(path: PathBuf, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn more_specific_settings_win_and_gaps_fall_through() {
        let dir = tempfile::tempdir().unwrap();
        let (repo, worktree, home) = (dir.path().join("r"), dir.path().join("w"), dir.path().join("h"));
        write(repo.join(".conductor/settings.local.toml"), "[scripts]\nsetup = \"local\"\n");
        write(
            worktree.join(".conductor/settings.toml"),
            "file_include_globs = \"a\\nb\\n\"\n[scripts]\nsetup = \"shared\"\narchive = \"shared-archive\"\n",
        );
        write(home.join(".conductor/settings.toml"), "[scripts]\nsetup = \"user\"\narchive = \"user-archive\"\n");
        let mut warnings = Vec::new();
        let settings = load_settings(&repo, &worktree, Some(&home), &mut warnings);
        assert_eq!(settings.setup.as_deref(), Some("local"));
        assert_eq!(settings.archive.as_deref(), Some("shared-archive"));
        assert_eq!(settings.file_include_globs.as_deref(), Some("a\nb\n"));
        assert!(warnings.is_empty());
    }

    #[test]
    fn a_broken_settings_file_is_a_warning_rather_than_a_failure() {
        let dir = tempfile::tempdir().unwrap();
        let (repo, worktree) = (dir.path().join("r"), dir.path().join("w"));
        write(repo.join(".conductor/settings.local.toml"), "not [ toml");
        write(worktree.join(".conductor/settings.toml"), "[scripts]\nsetup = \"shared\"\n");
        let mut warnings = Vec::new();
        let settings = load_settings(&repo, &worktree, None, &mut warnings);
        assert_eq!(settings.setup.as_deref(), Some("shared"));
        assert_eq!(warnings.len(), 1);
    }

    #[tokio::test]
    async fn by_default_only_ignored_env_files_are_copied() {
        let dir = tempfile::tempdir().unwrap();
        let (repo, worktree) = repo_and_worktree(dir.path(), ".env*\nsecret.txt\nnode_modules/\n");
        write(repo.join(".env.local"), "A=1\n");
        write(repo.join("secret.txt"), "ignored, but not an env file\n");
        write(repo.join("node_modules/x/.env.sample"), "deep\n");
        write(repo.join(".env-untracked-not-ignored-wont-exist"), "x");
        write(repo.join("work-in-progress.txt"), "not ignored, so it's the user's work\n");

        let mut warnings = Vec::new();
        let mut copied = copy_local_files(&repo, &worktree, &Settings::default(), &mut warnings).await;
        copied.sort();
        assert!(copied.contains(&".env.local".to_string()), "{copied:?}");
        assert_eq!(std::fs::read_to_string(worktree.join(".env.local")).unwrap(), "A=1\n");
        assert!(!worktree.join("secret.txt").exists());
        assert!(!worktree.join("work-in-progress.txt").exists());
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[tokio::test]
    async fn worktreeinclude_decides_what_is_copied() {
        let dir = tempfile::tempdir().unwrap();
        let (repo, worktree) = repo_and_worktree(dir.path(), ".env*\nconfig/*.local.json\n");
        write(repo.join(".worktreeinclude"), "# just this\nconfig/*.local.json\n");
        write(repo.join("config/app.local.json"), "{}\n");
        write(repo.join(".env"), "A=1\n");
        let copied = copy_local_files(&repo, &worktree, &Settings::default(), &mut Vec::new()).await;
        assert_eq!(copied, vec!["config/app.local.json".to_string()]);
        assert!(!worktree.join(".env").exists(), "an include file replaces the default, it doesn't add to it");
    }

    #[tokio::test]
    async fn the_settings_globs_are_used_when_there_is_no_include_file() {
        let dir = tempfile::tempdir().unwrap();
        let (repo, worktree) = repo_and_worktree(dir.path(), "certs/\n");
        write(repo.join("certs/local.pem"), "pem\n");
        let settings = Settings { file_include_globs: Some("certs/*.pem\n".into()), ..Default::default() };
        let copied = copy_local_files(&repo, &worktree, &settings, &mut Vec::new()).await;
        assert_eq!(copied, vec!["certs/local.pem".to_string()]);
    }

    #[tokio::test]
    async fn an_existing_file_is_never_overwritten_and_symlinks_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let (repo, worktree) = repo_and_worktree(dir.path(), ".env*\n");
        write(repo.join(".env"), "from the checkout\n");
        write(worktree.join(".env"), "already here\n");
        std::os::unix::fs::symlink("/etc/hosts", repo.join(".env.link")).unwrap();
        let copied = copy_local_files(&repo, &worktree, &Settings::default(), &mut Vec::new()).await;
        assert!(copied.is_empty(), "{copied:?}");
        assert_eq!(std::fs::read_to_string(worktree.join(".env")).unwrap(), "already here\n");
        assert!(!worktree.join(".env.link").exists());
    }

    #[tokio::test]
    async fn scripts_run_in_the_worktree_with_the_environment_and_log_their_output() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("setup.log");
        let env = vec![("CONDUCTOR_WORKSPACE_NAME".to_string(), "alder".to_string())];
        let status =
            run_script("echo out $CONDUCTOR_WORKSPACE_NAME; pwd; echo err >&2; exit 3", dir.path(), &env, &log, |_| {})
                .await
                .unwrap();
        assert_eq!(status.code(), Some(3));
        let text = std::fs::read_to_string(&log).unwrap();
        assert!(text.contains("out alder"), "{text}");
        assert!(text.contains("err"), "{text}");
        assert!(text.contains(dir.path().file_name().unwrap().to_str().unwrap()), "{text}");
    }

    #[test]
    fn the_log_tail_is_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("setup.log");
        std::fs::write(&log, "x".repeat(1000) + "end").unwrap();
        let tail = read_log_tail(&log, 10);
        assert_eq!(tail.len(), 10);
        assert!(tail.ends_with("end"));
        assert_eq!(read_log_tail(&dir.path().join("missing"), 10), "");
    }
}
