//! How long a doc and a table cell can be. A doc with room left takes one more rule every time something goes wrong,
//! so `docs/standards/documentation.md` gives each one a limit, and adding to a full doc means cutting or moving
//! something.

use std::collections::BTreeMap;
use std::path::Path;

/// The most words a doc can have, by its path or the folder it's in. A reference doc has no limit, since it's searched
/// rather than read through.
const LIMITS: &[(&str, usize)] = &[
    ("AGENTS.md", 800),
    ("docs/README.md", 1_500),
    ("docs/standards/", 2_000),
    ("docs/principles/", 1_500),
    ("docs/decisions/", 600),
];

/// Docs longer than that from before there was a limit, as (path, the most words it can have). A ceiling only comes
/// down: it stays within [`SLACK`] of the doc as the doc is shortened, and the doc comes off this list once it's within
/// its kind's limit, or gone.
const OVER: &[(&str, usize)] = &[
    ("docs/architecture.md", 46_200),
    ("docs/ui.md", 3_700),
    ("docs/standards/components.md", 4_500),
    ("docs/standards/gpui.md", 3_000),
    ("docs/standards/reviews.md", 3_100),
    ("docs/standards/testing.md", 2_100),
];

/// How far under its ceiling a doc on [`OVER`] can be before the ceiling has to come down to it.
const SLACK: usize = 300;

/// The most words a table's cell can have. A longer one is prose, which a table can't lay out.
const MAX_CELL_WORDS: usize = 60;

/// Docs with longer cells from before there was a limit, as (path, how many). The number only comes down.
const LONG_CELLS: &[(&str, usize)] = &[
    ("docs/architecture.md", 8),
    ("docs/reference/antigravity-cli.md", 7),
    ("docs/reference/claude-cli.md", 67),
    ("docs/reference/codex-cli.md", 7),
    ("docs/reference/terms.md", 55),
];

/// The folders whose docs these limits cover. The spec and the guide aren't among them.
const FOLDERS: &[&str] = &["docs/standards", "docs/principles", "docs/decisions", "docs/reference"];

/// The docs outside those folders that the limits cover.
const FILES: &[&str] = &["AGENTS.md", "docs/README.md", "docs/architecture.md", "docs/ui.md"];

/// Each covered doc that exists, as (path from the repo's root, its text).
fn docs() -> BTreeMap<String, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut paths: Vec<String> = FILES.iter().map(ToString::to_string).collect();
    for folder in FOLDERS {
        let Ok(entries) = std::fs::read_dir(root.join(folder)) else { continue };
        for entry in entries {
            let name = entry.unwrap().file_name().into_string().unwrap();
            if Path::new(&name).extension().is_some_and(|extension| extension == "md") {
                paths.push(format!("{folder}/{name}"));
            }
        }
    }
    paths
        .into_iter()
        .filter_map(|path| std::fs::read_to_string(root.join(&path)).ok().map(|text| (path, text)))
        .collect()
}

/// The limit for a doc at `path`, if its kind has one.
fn limit(path: &str) -> Option<usize> {
    LIMITS
        .iter()
        .find(|(prefix, _)| path == *prefix || (prefix.ends_with('/') && path.starts_with(prefix)))
        .map(|(_, most)| *most)
}

/// How many of `text`'s table cells have more than [`MAX_CELL_WORDS`] words. A `\|` is part of its cell.
fn long_cells(text: &str) -> usize {
    text.lines()
        .filter(|line| line.starts_with('|'))
        .flat_map(|line| {
            line.replace("\\|", " ").split('|').map(|cell| cell.split_whitespace().count()).collect::<Vec<_>>()
        })
        .filter(|words| *words > MAX_CELL_WORDS)
        .count()
}

#[test]
fn every_doc_stays_within_its_word_limit() {
    let docs = docs();
    assert!(docs.len() > 20, "found only {} docs, so this isn't looking in the right place", docs.len());
    let words: BTreeMap<&str, usize> =
        docs.iter().map(|(path, text)| (path.as_str(), text.split_whitespace().count())).collect();

    let over: Vec<_> = words
        .iter()
        .filter(|(path, _)| !OVER.iter().any(|(listed, _)| listed == *path))
        .filter_map(|(path, count)| {
            limit(path).filter(|most| count > most).map(|most| format!("{path} has {count} words, over {most}"))
        })
        .collect();
    assert!(over.is_empty(), "cut or move something, as `docs/standards/documentation.md` says: {over:?}");

    let grown: Vec<_> = OVER
        .iter()
        .filter_map(|(path, ceiling)| {
            words
                .get(path)
                .filter(|count| *count > ceiling)
                .map(|count| format!("{path} has {count} words, over {ceiling}"))
        })
        .collect();
    assert!(grown.is_empty(), "these were to get shorter, not longer: {grown:?}");

    let stale: Vec<_> = OVER
        .iter()
        .filter(|(path, ceiling)| match words.get(path) {
            None => true,
            Some(count) => count + SLACK < *ceiling || limit(path).is_some_and(|most| *count <= most),
        })
        .collect();
    assert!(
        stale.is_empty(),
        "shorter now, or gone, so lower these in `OVER` or take them off: {stale:?}, at {words:?}"
    );
}

#[test]
fn every_table_cell_stays_within_its_word_limit() {
    let cells: BTreeMap<String, usize> =
        docs().into_iter().map(|(path, text)| (path, long_cells(&text))).filter(|(_, count)| *count > 0).collect();

    let more: Vec<_> = cells
        .iter()
        .filter(|(path, count)| {
            LONG_CELLS.iter().find(|(listed, _)| listed == *path).is_none_or(|(_, allowed)| *count > allowed)
        })
        .collect();
    assert!(
        more.is_empty(),
        "a cell over {MAX_CELL_WORDS} words is prose, so write it as prose or shorten it: {more:?}"
    );

    let stale: Vec<_> =
        LONG_CELLS.iter().filter(|(path, allowed)| cells.get(*path).copied().unwrap_or(0) < *allowed).collect();
    assert!(
        stale.is_empty(),
        "fewer long cells now, so lower these in `LONG_CELLS` or take them off: {stale:?}, at {cells:?}"
    );
}

#[test]
fn a_cell_is_counted_by_its_words_and_keeps_an_escaped_bar() {
    let long = vec!["word"; MAX_CELL_WORDS + 1].join(" ");
    let short = vec!["word"; MAX_CELL_WORDS].join(" ");

    assert_eq!(long_cells(&format!("| {short} | {long} |\n| {long} | a |\nNot a row, though long: {long}")), 2);
    assert_eq!(long_cells(&format!("| {short} \\| x | a |")), 1, "an escaped bar doesn't end the cell");
}
