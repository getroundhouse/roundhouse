//! Builds the Rust stand-in once per cargo-test process or nextest run.
#![cfg(test)]
use std::path::PathBuf;
use std::sync::OnceLock;

pub(super) fn program() -> PathBuf {
    static PROGRAM: OnceLock<PathBuf> = OnceLock::new();
    PROGRAM.get_or_init(build).clone()
}

fn build() -> PathBuf {
    let dir = option_env!("CARGO_TARGET_TMPDIR")
        .map_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp"), PathBuf::from);
    std::fs::create_dir_all(&dir).unwrap();
    let lock = std::fs::File::create(dir.join("codex-native-standin.lock")).unwrap();
    lock.lock().unwrap();
    let stamp = dir.join("codex-native-standin.json");
    let run = std::env::var("NEXTEST_RUN_ID").ok();
    if let Some(run) = &run
        && let Ok(bytes) = std::fs::read(&stamp)
        && let Ok((last, path)) = serde_json::from_slice::<(String, PathBuf)>(&bytes)
        && last == *run
    {
        return path;
    }
    let output = std::process::Command::new(env!("CARGO"))
        .args(["build", "-p", "agents", "--example", "native_standin", "--locked", "--message-format=json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let path = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|message| message["reason"] == "compiler-artifact" && message["target"]["name"] == "native_standin")
        .and_then(|message| message["executable"].as_str().map(PathBuf::from))
        .unwrap();
    if let Some(run) = run {
        std::fs::write(stamp, serde_json::to_vec(&(run, &path)).unwrap()).unwrap();
    }
    path
}
