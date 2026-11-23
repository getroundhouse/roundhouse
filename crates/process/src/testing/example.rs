//! A program in `tools/stand-ins/` that a test runs in place of another, built as the test first asks for it.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use super::workspace_build::{once_per_run, workspace_build};

/// One example, built once per test process, and once per nextest run, as [`once_per_run`] says.
pub struct Example {
    name: &'static str,
    program: OnceLock<PathBuf>,
}

impl Example {
    pub const fn new(name: &'static str) -> Self {
        Self { name, program: OnceLock::new() }
    }

    /// Where Cargo put it, once it's built.
    ///
    /// # Panics
    ///
    /// If the build fails or doesn't say where it put it.
    pub fn path(&self) -> PathBuf {
        self.program.get_or_init(|| build(self.name)).clone()
    }
}

/// Builds the example `name` with [`workspace_build`], and answers where Cargo put it. The lock and what says where are
/// kept in the workspace's `target/tmp`, since Cargo sets `CARGO_TARGET_TMPDIR` only for a package's tests in `tests/`.
fn build(name: &str) -> PathBuf {
    let failed = |doing: &str, e: &dyn std::fmt::Display| -> ! { panic!("couldn't {doing} for {name}: {e}") };
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    let said = dir.join(format!("{name}.path"));
    once_per_run(&dir, name, || {
        let mut build = workspace_build();
        build.args(["--example", name, "--locked", "--message-format=json-render-diagnostics"]);
        let output = build.output().unwrap_or_else(|e| failed("run cargo", &e));
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let path = (output.stdout.split(|&byte| byte == b'\n'))
            .filter_map(|line| serde_json::from_slice::<serde_json::Value>(line).ok())
            .find(|message| message["reason"] == "compiler-artifact" && message["target"]["name"] == name)
            .and_then(|message| Some(PathBuf::from(message["executable"].as_str()?)))
            .unwrap_or_else(|| failed("find where Cargo built it", &"it didn't say"));
        std::fs::write(&said, path.as_os_str().as_encoded_bytes()).unwrap_or_else(|e| failed("keep where", &e));
    });
    PathBuf::from(std::fs::read_to_string(&said).unwrap_or_else(|e| failed("read where", &e)))
}
