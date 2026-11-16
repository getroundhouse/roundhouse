use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

use super::{HintGraph, HintState, Hints};

/// Merged subwords first (in BPE merge order), followed by the single-character alphabet (including `▁`), just as
/// `SentencePiece` BPE `vocab.json` orders them.
fn sample_vocab() -> Vec<String> {
    [
        "<unk>", "▁A", "▁Ab", "bc", "▁T", "ok", "io", "▁h", "▁hu", "▁H", "▁Hu", "▁c", "▁cr", "at", "ate", "▁C", "▁cat",
        "▁Cat", "af", "aft", "▁w", "▁wh", "ol", "ole", "eg", "or", "ory", "▁i", "▁is", "▁r", "▁R", "us", "ust", "▁",
        "A", "C", "H", "R", "T", "a", "b", "c", "e", "f", "g", "h", "i", "k", "l", "m", "o", "r", "s", "t", "u", "w",
        "y", ".",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

fn id(vocab: &[String], piece: &str) -> usize {
    vocab.iter().position(|p| p == piece).unwrap_or_else(|| panic!("missing piece {piece:?}"))
}

fn ids(vocab: &[String], pieces: &[&str]) -> Vec<usize> {
    pieces.iter().map(|p| id(vocab, p)).collect()
}

fn score_sequence(graph: &HintGraph, tokens: &[usize]) -> f32 {
    let mut state = HintState::default();
    let mut total = 0.0;
    for &token in tokens {
        let (next, delta) = graph.step(state, token);
        state = next;
        total += delta;
    }
    total + graph.finish(state)
}

#[test]
fn hints_clean_deduplicate_and_expand_identifiers() {
    let hints = Hints::new([
        " `Tokio` ",
        "tokio",
        "workspace_pane",
        "WorkspacePane",
        "CBLAS_SGEMM",
        "SQLite",
        "a",
        "1234",
        "deadbeefcafe01",
    ]);

    assert_eq!(hints.words(), ["Tokio", "workspace pane", "WorkspacePane", "cblas sgemm", "SQLite"]);
}

#[test]
fn whole_hint_keeps_its_boost_while_partial_and_mid_word_matches_roll_back_to_zero() {
    let v = sample_vocab();
    let hints = Hints::new(["Tokio", "hub crate", "cat", "rustfmt", "Abc"]);
    let graph = HintGraph::new(&v, &hints, 1.5);

    // "Tokio." -> 3 tokens * 1.5 = 4.5, banked before "."
    assert!((score_sequence(&graph, &ids(&v, &["▁T", "ok", "io", "."])) - 4.5).abs() < 1e-5);
    // "Tokyo" shares "▁T" + "ok" (+3.0) then diverges on "y" (-3.0) -> 0.0
    assert!(score_sequence(&graph, &ids(&v, &["▁T", "ok", "y", "o"])).abs() < 1e-5);
    // "hub crafted" matches 3 of 4 tokens in "hub crate" (+4.5) then diverges on "aft" -> 0.0
    assert!(score_sequence(&graph, &ids(&v, &["▁hu", "b", "▁cr", "aft"])).abs() < 1e-5);
    // "category" matches "▁cat" (+1.5) then continues mid-word with "eg" -> rolls back to 0.0
    assert!(score_sequence(&graph, &ids(&v, &["▁cat", "eg", "ory"])).abs() < 1e-5);
    // "cat is" ends "▁cat" at word boundary "▁is" -> keeps +1.5
    assert!((score_sequence(&graph, &ids(&v, &["▁cat", "▁is"])) - 1.5).abs() < 1e-5);
    // Sentence-initial "Rustfmt" matches lowercase hint "rustfmt" -> 5 * 1.5 = 7.5
    assert!((score_sequence(&graph, &ids(&v, &["▁R", "ust", "f", "m", "t"])) - 7.5).abs() < 1e-5);
    // BPE merge rank: "▁Ab" (rank 2) beats "bc" (rank 3) even though "▁A" + "bc" has smaller rank sum (1 + 3 < 2 + 41)
    assert!((score_sequence(&graph, &ids(&v, &["▁Ab", "c"])) - 3.0).abs() < 1e-5);
    assert!(score_sequence(&graph, &ids(&v, &["▁A", "bc"])).abs() < 1e-5);
}

#[test]
fn completed_suffix_hint_banks_its_boost_when_an_enclosing_multi_word_hint_diverges() {
    let v = sample_vocab();
    let hints = Hints::new(["hub cat whole", "cat"]);
    let graph = HintGraph::new(&v, &hints, 2.0);

    // "hub cat is": "hub cat whole" diverges at "▁is", but suffix "cat" completed at that boundary -> keeps +2.0
    assert!((score_sequence(&graph, &ids(&v, &["▁hu", "b", "▁cat", "▁is"])) - 2.0).abs() < 1e-5);
    // "hub cat" at end-of-piece: "hub" rolls back while suffix "cat" keeps +2.0
    assert!((score_sequence(&graph, &ids(&v, &["▁hu", "b", "▁cat"])) - 2.0).abs() < 1e-5);
    // "hub category": "▁cat" continues mid-word with "eg", so neither hint completed -> 0.0
    assert!(score_sequence(&graph, &ids(&v, &["▁hu", "b", "▁cat", "eg", "ory"])).abs() < 1e-5);
}

proptest! {
    #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

    #[test]
    fn sequences_without_complete_hint_words_always_settle_to_zero(
        tokens in prop::collection::vec(1usize..58, 0..30)
    ) {
        let vocab = sample_vocab();
        let hints = Hints::new(["Tokio", "hub crate", "cat", "rustfmt"]);
        let graph = HintGraph::new(&vocab, &hints, 2.0);
        let text: String = tokens.iter().map(|&id| vocab[id].as_str()).collect::<String>().replace('▁', " ");
        let words: Vec<&str> = text.split_whitespace().map(|w| w.trim_matches('.')).collect();
        let has_hint = words.iter().any(|&w| matches!(w, "Tokio" | "cat" | "Cat" | "rustfmt" | "Rustfmt"))
            || text.contains(" hub crate ")
            || text.ends_with(" hub crate")
            || text.contains(" Hub crate ")
            || text.ends_with(" Hub crate");
        if !has_hint {
            prop_assert!((score_sequence(&graph, &tokens)).abs() < 1e-4, "text={text:?} tokens={tokens:?}");
        }
    }
}
