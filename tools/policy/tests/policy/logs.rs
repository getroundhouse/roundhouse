//! The rules for what's logged, in `docs/principles/observability.md`: an event has a name, a fixed message and its
//! values as fields, and which crates' events are ours.

use std::collections::BTreeSet;
use std::path::Path;

use proc_macro2::{Delimiter, Spacing, TokenStream, TokenTree};

use super::rust_standards::{add_rust_files, covered_crates};
use super::workspace::workspace;

/// The macros that log an event.
const LOGGING: &[&str] = &["error", "warn", "info", "debug", "trace"];

/// The events logged in `code`, which is a file's tokens, that don't keep the rule, each as it's written.
fn loose_events(code: TokenStream) -> Vec<String> {
    let tokens: Vec<TokenTree> = code.into_iter().collect();
    let mut found = Vec::new();
    for (at, token) in tokens.iter().enumerate() {
        if let TokenTree::Group(group) = token {
            found.extend(loose_events(group.stream()));
        }
        if let TokenTree::Ident(name) = token
            && LOGGING.iter().any(|logging| name == logging)
            && let [TokenTree::Punct(bang), TokenTree::Group(event), ..] = &tokens[at + 1..]
            && bang.as_char() == '!'
            && !keeps_the_rule(event.stream())
        {
            found.push(format!("{name}!({})", event.stream()));
        }
    }
    found
}

/// Whether an event's inside, `event`, starts with its name, and ends with a message that's a string with no value in
/// it.
fn keeps_the_rule(event: TokenStream) -> bool {
    let tokens: Vec<TokenTree> = event.into_iter().collect();
    let named = match tokens.as_slice() {
        [TokenTree::Ident(name), TokenTree::Punct(colon), TokenTree::Literal(value), ..] => {
            name == "name"
                && colon.as_char() == ':'
                && colon.spacing() == Spacing::Alone
                && is_a_name(&value.to_string())
        }
        _ => false,
    };
    let trailing = |token: &TokenTree| matches!(token, TokenTree::Punct(comma) if comma.as_char() == ',');
    let parts: Vec<&[TokenTree]> = tokens.split(trailing).filter(|part| !part.is_empty()).collect();
    let said = match parts.last() {
        Some([TokenTree::Literal(message)]) => {
            let message = message.to_string();
            message.starts_with('"') && !message.contains(['{', '}'])
        }
        _ => false,
    };
    named && said
}

/// Whether `literal` is a string of parts such as `"github.read.failed"`, which says what happened and to what.
fn is_a_name(literal: &str) -> bool {
    let Some(name) = literal.strip_prefix('"').and_then(|rest| rest.strip_suffix('"')) else { return false };
    let part = |part: &str| !part.is_empty() && part.chars().all(|c| c.is_ascii_lowercase() || c == '_');
    name.contains('.') && name.split('.').all(part)
}

#[test]
fn an_event_keeps_the_rule_with_a_name_a_fixed_message_and_its_values_as_fields() {
    let loose = |code: &str| loose_events(code.parse().unwrap()).len();

    for kept in [
        r#"warn!(name: "github.read.failed", %workspace_id, ?error, tries = 2, "Couldn't read pull requests");"#,
        r#"tracing::info!(name: "app.started", "Started");"#,
        r#"if failed { error!(name: "store.write_failed", error = f(a, b), "Couldn't write",); }"#,
        r#"let text = "info!(said)"; // warn!("{said}")"#,
        "macro_rules! warn { () => {} }",
    ] {
        assert_eq!(loose(kept), 0, "{kept}");
    }
    for broken in [
        r#"warn!("couldn't read pull requests");"#,
        r#"warn!(name: "failed", "Couldn't read");"#,
        r#"warn!(name: "github.read.Failed", "Couldn't read");"#,
        r#"warn!(name: "github.read.failed", "Couldn't read: {error}");"#,
        r#"warn!(name: "github.read.failed", "Couldn't read: {}", error);"#,
        r#"warn!(name: "github.read.failed", %error);"#,
        r#"warn!(name: "github.read.failed");"#,
        r#"match read { Err(e) => tracing::debug!(%e, "couldn't read"), Ok(_) => {} }"#,
    ] {
        assert_eq!(loose(broken), 1, "{broken}");
    }
    assert_eq!(loose(r#"warn!(name: "a.b", "Couldn't", {warn!("inside")});"#), 2, "one in another");
}

#[test]
fn every_event_has_a_name_a_fixed_message_and_its_values_as_fields() {
    let metadata = workspace();
    let root = Path::new(metadata["workspace_root"].as_str().unwrap());
    let mut loose = Vec::new();
    let crates = covered_crates(&metadata);
    assert!(crates.len() > 10, "found only {} crates, so this isn't looking in the right place", crates.len());
    for dir in crates {
        let mut files = Vec::new();
        add_rust_files(&dir, &mut files);
        for file in files.iter().filter(|file| !is_a_test(file.strip_prefix(&dir).unwrap())) {
            let code: TokenStream = std::fs::read_to_string(file).unwrap().parse().unwrap();
            let found = loose_events(code);
            if !found.is_empty() {
                loose.push(format!("{}: {found:#?}", file.strip_prefix(root).unwrap().display()));
            }
        }
    }
    assert!(
        loose.is_empty(),
        "give each event a name, a message with no value in it, and its values as fields: {loose:#?}"
    );
}

/// Whether the file at `path`, in its crate, is a test's, which can log what it likes.
fn is_a_test(path: &Path) -> bool {
    path.components().any(|part| part.as_os_str() == "tests" || part.as_os_str() == "tests.rs")
}

/// Every crate that logs, by what its events are from, which is its library's or its program's name, is one
/// `logging::OURS` has at `info`.
#[test]
fn every_crate_that_logs_is_one_of_ours() {
    let metadata = workspace();
    let ours: BTreeSet<String> = ours(
        &std::fs::read_to_string(
            Path::new(metadata["workspace_root"].as_str().unwrap()).join("crates/logging/src/levels.rs"),
        )
        .unwrap(),
    );
    let logs = |package: &&serde_json::Value| {
        package["dependencies"].as_array().unwrap().iter().any(|on| on["name"] == "tracing" && on["kind"].is_null())
    };
    let members: BTreeSet<&str> =
        metadata["workspace_members"].as_array().unwrap().iter().map(|id| id.as_str().unwrap()).collect();
    let from: BTreeSet<String> = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|package| members.contains(package["id"].as_str().unwrap()))
        .filter(logs)
        .flat_map(|package| {
            let targets = package["targets"].as_array().unwrap();
            let of = |kind: &'static str| {
                targets.iter().filter(move |target| target["kind"].as_array().unwrap().iter().any(|is| is == kind))
            };
            // A crate with a library logs from it, and its program only starts it.
            let from: Vec<_> = if of("lib").next().is_some() { of("lib").collect() } else { of("bin").collect() };
            from.into_iter().map(|target| target["name"].as_str().unwrap().replace('-', "_"))
        })
        .collect();
    assert_eq!(ours, from, "`logging::OURS` has each crate that logs, and no other");
}

/// The strings in the list `OURS` is, in `levels`, the file that has it.
fn ours(levels: &str) -> BTreeSet<String> {
    let tokens: Vec<TokenTree> = levels.parse::<TokenStream>().unwrap().into_iter().collect();
    let named = tokens.iter().position(|token| matches!(token, TokenTree::Ident(name) if name == "OURS")).unwrap();
    let list = tokens[named..].iter().find_map(|token| match token {
        // After the type, which is in brackets of its own.
        TokenTree::Group(group) if group.delimiter() == Delimiter::Bracket => {
            let strings: BTreeSet<String> = group
                .stream()
                .into_iter()
                .filter_map(|token| match token {
                    TokenTree::Literal(literal) => Some(literal.to_string().trim_matches('"').to_owned()),
                    TokenTree::Group(_) | TokenTree::Ident(_) | TokenTree::Punct(_) => None,
                })
                .collect();
            (!strings.is_empty()).then_some(strings)
        }
        TokenTree::Group(_) | TokenTree::Ident(_) | TokenTree::Punct(_) | TokenTree::Literal(_) => None,
    });
    list.unwrap()
}
