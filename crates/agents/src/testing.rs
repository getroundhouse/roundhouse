//! Builds native Rust stand-ins for adapter tests on every supported platform.
#![expect(clippy::unwrap_used, reason = "test support whose failure must fail the calling test")]
use std::path::PathBuf;
use std::sync::OnceLock;

/// One example binary, initialized once per test process, with a cross-process nextest stamp.
pub struct Example {
    name: &'static str,
    program: OnceLock<PathBuf>,
}

impl Example {
    pub const fn new(name: &'static str) -> Self {
        Self { name, program: OnceLock::new() }
    }

    pub fn path(&self) -> PathBuf {
        self.program.get_or_init(|| build(self.name)).clone()
    }
}

fn build(name: &str) -> PathBuf {
    let dir = option_env!("CARGO_TARGET_TMPDIR")
        .map_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp"), PathBuf::from);
    std::fs::create_dir_all(&dir).unwrap();
    let lock = std::fs::File::create(dir.join(format!("{name}.lock"))).unwrap();
    lock.lock().unwrap();
    let stamp = dir.join(format!("{name}.json"));
    let run = std::env::var("NEXTEST_RUN_ID").ok();
    if let Some(run) = &run
        && let Ok(bytes) = std::fs::read(&stamp)
        && let Ok((last, path)) = serde_json::from_slice::<(String, PathBuf)>(&bytes)
        && last == *run
    {
        return path;
    }
    // With the whole workspace's features, as `docs/rust.md` says.
    let output = std::process::Command::new(env!("CARGO"))
        .args(["build", "--workspace", "--example", name, "--locked", "--message-format=json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let path = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|message| message["reason"] == "compiler-artifact" && message["target"]["name"] == name)
        .and_then(|message| message["executable"].as_str().map(PathBuf::from))
        .unwrap();
    if let Some(run) = run {
        std::fs::write(stamp, serde_json::to_vec(&(run, &path)).unwrap()).unwrap();
    }
    path
}
