//! Made-up prose, for `prose:`, which writes a long reply.

/// `paragraphs` paragraphs of made-up prose, the same for the same `seed`, of lengths as a reply's vary: some a line,
/// some several, with now and then a list or a block of code. One chunk to a paragraph.
pub(super) fn prose(paragraphs: usize, seed: u64) -> Vec<String> {
    const WORDS: [&str; 16] = [
        "signal", "box", "keeps", "the", "ledger", "of", "every", "train", "that", "passes", "and", "lever", "frame",
        "holds", "route", "clear",
    ];
    let mut state = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
    let mut next = move |n: u64| {
        state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        usize::try_from((state >> 33) % n).unwrap()
    };
    Vec::from_fn(paragraphs, |_| {
        let words = [6, 14, 30, 60, 120][next(5)];
        let sentence = Vec::from_fn(words, |_| WORDS[next(16)]);
        let text = sentence.join(" ");
        match next(8) {
            0 => format!("- {}\n- {}\n- {}\n\n", WORDS[next(16)], text, WORDS[next(16)]),
            1 => format!("```rust\nfn {}() {{\n    {}\n}}\n```\n\n", WORDS[next(16)], text),
            _ => format!("{text}.\n\n"),
        }
    })
}
