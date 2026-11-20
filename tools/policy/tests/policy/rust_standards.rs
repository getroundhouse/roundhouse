//! The Rust standards every crate keeps: how long a file can be, where its modules' code is, and the workspace's lints.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use proc_macro2::{Delimiter, TokenStream, TokenTree};
use serde_json::Value;

use super::workspace::workspace;

/// The most lines a Rust file can have, tests included. `docs/standards/rust.md` has the reasoning.
const MAX_FILE_LINES: usize = 400;

/// Files allowed past that limit, as (path from the repo root, why). Empty so far.
const LONG_FILES: &[(&str, &str)] = &[];

/// The directory of each crate the Rust standards cover, which is every workspace member.
pub(super) fn covered_crates(metadata: &Value) -> Vec<PathBuf> {
    let members: HashSet<&str> =
        metadata["workspace_members"].as_array().unwrap().iter().map(|id| id.as_str().unwrap()).collect();
    metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|package| members.contains(package["id"].as_str().unwrap()))
        .map(|package| Path::new(package["manifest_path"].as_str().unwrap()).parent().unwrap().to_path_buf())
        .collect()
}

pub(super) fn add_rust_files(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            add_rust_files(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

#[test]
fn every_file_stays_within_the_length_limit() {
    let metadata = workspace();
    let root = Path::new(metadata["workspace_root"].as_str().unwrap());
    let mut files = Vec::new();
    for dir in covered_crates(&metadata) {
        add_rust_files(&dir, &mut files);
    }
    assert!(files.len() > 100, "found only {} files, so this isn't looking in the right place", files.len());
    let long: BTreeMap<String, usize> = files
        .iter()
        .map(|file| (file, std::fs::read_to_string(file).unwrap().lines().count()))
        .filter(|(_, lines)| *lines > MAX_FILE_LINES)
        .map(|(file, lines)| (file.strip_prefix(root).unwrap().display().to_string(), lines))
        .collect();
    let unexplained: Vec<_> =
        long.iter().filter(|(path, _)| !LONG_FILES.iter().any(|(file, _)| file == path)).collect();
    assert!(unexplained.is_empty(), "over {MAX_FILE_LINES} lines, so split by responsibility: {unexplained:?}");
    let stale: Vec<_> = LONG_FILES.iter().filter(|(file, _)| !long.contains_key(*file)).collect();
    assert!(stale.is_empty(), "within the limit now, so take these off `LONG_FILES`: {stale:?}");
}

/// What a file's code does that `docs/standards/rust.md` refuses, since it takes code from somewhere a module's name
/// doesn't say.
#[derive(Debug, PartialEq, Eq)]
enum Reach {
    /// An attribute names a module's file, as `path = "…"` does, on its own or in a `cfg_attr`.
    NamesAFile,
    /// `include!` takes a file in as source, rather than what a build script wrote to `OUT_DIR`.
    IncludesSource,
}

/// Each reach in `code`, which is a file's tokens, so a string or a comment that looks like one isn't, and what a
/// macro's body holds is found too.
fn reaches(code: TokenStream) -> Vec<Reach> {
    let tokens: Vec<TokenTree> = code.into_iter().collect();
    let mut found = Vec::new();
    for (at, token) in tokens.iter().enumerate() {
        match token {
            TokenTree::Group(group) => {
                let attribute = match &tokens[..at] {
                    [.., TokenTree::Punct(hash), TokenTree::Punct(bang)] if bang.as_char() == '!' => {
                        hash.as_char() == '#'
                    }
                    [.., TokenTree::Punct(hash)] => hash.as_char() == '#',
                    _ => false,
                };
                if attribute && group.delimiter() == Delimiter::Bracket && names_a_file(group.stream()) {
                    found.push(Reach::NamesAFile);
                }
                found.extend(reaches(group.stream()));
            }
            TokenTree::Ident(name) if name == "include" => {
                if let [TokenTree::Punct(bang), TokenTree::Group(input), ..] = &tokens[at + 1..]
                    && bang.as_char() == '!'
                    && !written_by_a_build_script(input.stream())
                {
                    found.push(Reach::IncludesSource);
                }
            }
            TokenTree::Ident(_) | TokenTree::Punct(_) | TokenTree::Literal(_) => {}
        }
    }
    found
}

/// Whether an attribute's inside, `attribute`, names a file: `path = …`, or a `cfg_attr` that holds one.
fn names_a_file(attribute: TokenStream) -> bool {
    let tokens: Vec<TokenTree> = attribute.into_iter().collect();
    match tokens.as_slice() {
        [TokenTree::Ident(name), TokenTree::Punct(equals), ..] => name == "path" && equals.as_char() == '=',
        [TokenTree::Ident(name), TokenTree::Group(inside)] if name == "cfg_attr" => {
            let inside: Vec<TokenTree> = inside.stream().into_iter().collect();
            // Each attribute after the condition.
            inside
                .split(|token| matches!(token, TokenTree::Punct(comma) if comma.as_char() == ','))
                .skip(1)
                .any(|attribute| names_a_file(attribute.iter().cloned().collect()))
        }
        _ => false,
    }
}

/// Whether `include!`'s input, `input`, is a file in `OUT_DIR`, as `concat!(env!("OUT_DIR"), …)` is.
fn written_by_a_build_script(input: TokenStream) -> bool {
    let opens = |stream: TokenStream, name: &str| match stream.into_iter().collect::<Vec<_>>().as_slice() {
        [TokenTree::Ident(called), TokenTree::Punct(bang), TokenTree::Group(inside), ..]
            if called == name && bang.as_char() == '!' =>
        {
            Some(inside.stream())
        }
        _ => None,
    };
    let named = opens(input, "concat").and_then(|concatenated| opens(concatenated, "env"));
    named.is_some_and(|variable| variable.to_string() == r#""OUT_DIR""#)
}

/// Each reach in the covered crates' Rust, with the file it's in, from the repo root.
fn reaches_in_the_workspace() -> Vec<(String, Reach)> {
    let metadata = workspace();
    let root = Path::new(metadata["workspace_root"].as_str().unwrap());
    let mut files = Vec::new();
    for dir in covered_crates(&metadata) {
        add_rust_files(&dir, &mut files);
    }
    assert!(files.len() > 100, "found only {} files, so this isn't looking in the right place", files.len());
    let mut found = Vec::new();
    for file in files {
        let path = file.strip_prefix(root).unwrap().display().to_string();
        let code: TokenStream = std::fs::read_to_string(&file).unwrap().parse().unwrap();
        found.extend(reaches(code).into_iter().map(|reach| (path.clone(), reach)));
    }
    found
}

#[test]
fn a_reach_is_found_in_code_but_not_in_a_string_or_a_comment() {
    let reaches = |code: &str| reaches(code.parse().unwrap());

    assert_eq!(reaches(r#"#[path = "../build/res.rs"] mod res;"#), [Reach::NamesAFile]);
    assert_eq!(reaches(r#"#[cfg_attr(test, inline, path = "other.rs")] mod other;"#), [Reach::NamesAFile]);
    assert_eq!(reaches(r#"cfg_if! { if #[cfg(unix)] { include!("unix.rs"); } }"#), [Reach::IncludesSource]);
    assert_eq!(reaches(r#"include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../x.rs"));"#), [Reach::IncludesSource]);
    for fine in [
        r#"include!(concat!(env!("OUT_DIR"), "/file_icons.rs"));"#,
        r#"const SPEC: &str = include_str!("../../docs/spec/look.md");"#,
        r##"const CODE: &str = r#"#[path = "c.rs"] mod c; include!("d.rs");"#;"##,
        "/// Nothing names a module's file with `#[path]`.
fn f() {}",
        r#"#[serde(rename = "path")] struct S;"#,
        r#"let path = "x";"#,
    ] {
        assert_eq!(reaches(fine), [], "{fine}");
    }
}

/// A module is the file its name says, in its own package, as `docs/standards/rust.md` says, so another crate's code
/// comes through a dependency, which Cargo and the policy tests see.
#[test]
fn every_module_is_the_file_its_name_says() {
    let found = reaches_in_the_workspace();

    assert!(
        found.is_empty(),
        "move the code to where its module's name says, or into a crate both depend on: {found:?}"
    );
}

#[test]
fn every_crate_inherits_the_lints() {
    let metadata = workspace();
    let crates = covered_crates(&metadata);
    assert!(crates.len() > 10, "found only {} crates, so this isn't looking in the right place", crates.len());
    let missing: Vec<_> = crates
        .iter()
        .filter(|dir| {
            // Cargo's metadata leaves out `[lints]`, so this reads the manifest.
            let manifest: toml::Table = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap().parse().unwrap();
            manifest.get("lints").and_then(|lints| lints.get("workspace")).and_then(toml::Value::as_bool) != Some(true)
        })
        .collect();
    assert!(missing.is_empty(), "these need `[lints] workspace = true`: {missing:?}");
}
