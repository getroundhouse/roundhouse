//! What the worker's tests give the processes the product's code starts. `Environment::NONE` adds nothing to what a
//! process inherits, so the worker's git, run from a test with it, would read the environment of whoever runs the
//! tests, such as their git config, their hooks or a `GIT_DIR` from a hook they're in. A test gives it the worker's
//! `testing::given()` instead, as `docs/testing.md` says.

use std::path::Path;

use super::rust_standards::add_rust_files;
use super::workspace::workspace;

/// What a test mustn't give the product's processes.
const NOTHING_ADDED: &str = "Environment::NONE";

/// The parts of `text`, a Rust file, that are test code: the whole of a file of tests, such as a `tests.rs`, one under
/// `tests/`, or the worker's `testing.rs`, which only its tests build, and otherwise each module declared
/// `#[cfg(test)]` in the file itself, to the brace that closes it.
/// The product's code around such a module, above or below it, isn't, nor is a `#[cfg(test)]` on anything but a
/// module written out in the file. A brace in a string inside the module can end it in the wrong place, which none
/// of the worker's does.
fn test_code<'a>(path: &Path, text: &'a str) -> Vec<&'a str> {
    let named_tests = path.file_name().is_some_and(|name| name == "tests.rs" || name == "testing.rs");
    if named_tests || path.components().any(|part| part.as_os_str() == "tests") {
        return vec![text];
    }
    let mut parts = Vec::new();
    for (at, _) in text.match_indices("#[cfg(test)]") {
        let after = text[at + "#[cfg(test)]".len()..].trim_start();
        let Some(module) = after.strip_prefix("mod ").or_else(|| after.strip_prefix("pub(crate) mod ")) else {
            continue;
        };
        let (Some(open), semicolon) = (module.find('{'), module.find(';')) else { continue };
        if semicolon.is_some_and(|semicolon| semicolon < open) {
            continue;
        }
        let body = &module[open..];
        let mut depth = 0;
        let end = body.char_indices().find_map(|(i, c)| {
            depth += match c {
                '{' => 1,
                '}' => -1,
                _ => 0,
            };
            (depth == 0).then_some(i + 1)
        });
        parts.push(&body[..end.unwrap_or(body.len())]);
    }
    parts
}

#[test]
fn the_worker_s_tests_give_the_product_s_processes_what_a_test_gives() {
    let metadata = workspace();
    let root = Path::new(metadata["workspace_root"].as_str().unwrap());
    let mut files = Vec::new();
    add_rust_files(&root.join("crates/worker"), &mut files);
    assert!(files.len() > 50, "found only {} files, so this isn't looking in the right place", files.len());
    let mut found = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap();
        if test_code(file, &text).iter().any(|part| part.contains(NOTHING_ADDED)) {
            found.push(file.strip_prefix(root).unwrap().display().to_string());
        }
    }
    assert!(
        found.is_empty(),
        "a worker test gives what the product's code starts `crate::testing::given()`, or builds on it, rather than \
         {NOTHING_ADDED}, which leaves it the environment of whoever runs the tests, as docs/testing.md says: \
         {found:#?}"
    );
}

#[test]
fn test_code_is_a_file_of_tests_or_a_module_written_out_under_cfg_test() {
    let product = "use a;\n#[cfg(test)]\nuse b;\n#[cfg(test)]\nmod tests;\nfn product() { x }\n";
    let inline = "fn product() { x }\n#[cfg(test)]\nmod tests {\n    fn a() { if y { z } }\n}\nfn after() { w }\n";

    assert!(test_code(Path::new("src/a.rs"), product).is_empty());
    assert_eq!(test_code(Path::new("src/a.rs"), inline), ["{\n    fn a() { if y { z } }\n}"]);
    assert_eq!(test_code(Path::new("src/a/tests.rs"), product), [product]);
    assert_eq!(test_code(Path::new("tests/serve/main.rs"), product), [product]);
}
