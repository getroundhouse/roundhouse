//! An Aho-Corasick graph over subword tokens that gives a provisional boost as a candidate matches a hint, banks it
//! at a word boundary once the whole hint matched, and subtracts it back out when what follows diverges.

use std::collections::{HashMap, VecDeque};

use super::Hints;

/// What starts a word in `SentencePiece` vocabularies.
const WORD_START: char = '\u{2581}';

/// Where a candidate transcript stands in [`HintGraph`], and how many matched tokens haven't been banked at a word
/// boundary yet. Integer counts avoid floating-point drift when rolling a partial match back.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct HintState {
    node: u32,
    unbanked: u16,
}

#[derive(Debug)]
struct Node {
    /// Sorted by token ID.
    children: Vec<(usize, u32)>,
    fail: u32,
    depth: u16,
    is_end: bool,
}

/// Tokenized hint phrases with failure links, built from any subword vocabulary whose word starts begin with `▁`.
#[derive(Debug)]
pub(crate) struct HintGraph {
    nodes: Vec<Node>,
    /// Whether each token ID can end the word before it: either starts a new word (`▁…`) or is punctuation.
    word_boundary: Vec<bool>,
    /// Whether each token ID is a control piece such as `<unk>`.
    ignored: Vec<bool>,
    boost: f32,
}

impl HintGraph {
    /// Builds the graph for `hints` over `pieces` (indexed by token ID), awarding `boost` per matched token.
    pub(crate) fn new(pieces: &[String], hints: &Hints, boost: f32) -> Self {
        let word_boundary = pieces.iter().map(|piece| is_word_boundary(piece)).collect();
        let ignored = pieces.iter().map(|piece| is_control(piece)).collect();
        let mut nodes = vec![Node { children: Vec::new(), fail: 0, depth: 0, is_end: false }];
        if hints.is_empty() || pieces.is_empty() || boost <= 0.0 {
            return Self { nodes, word_boundary, ignored, boost: 0.0 };
        }
        let by_piece = index_pieces(pieces);
        for phrase in hints.words() {
            for form in cased_forms(phrase) {
                if let Some(tokens) = tokenize(&form, &by_piece) {
                    insert(&mut nodes, &tokens);
                }
            }
        }
        if nodes.len() > 1 {
            connect_failures(&mut nodes);
        }
        Self { nodes, word_boundary, ignored, boost }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.nodes.len() <= 1 || self.boost <= 0.0
    }

    /// Advances from `state` on `token`, returning the next state and the score change to add to the beam.
    pub(crate) fn step(&self, state: HintState, token: usize) -> (HintState, f32) {
        if self.is_empty() {
            return (state, 0.0);
        }
        if self.ignored.get(token).copied().unwrap_or(true) {
            return (HintState::default(), -f32::from(state.unbanked) * self.boost);
        }
        let boundary = self.word_boundary.get(token).copied().unwrap_or(false);
        let current = &self.nodes[state.node as usize];
        if current.is_end && boundary {
            return self.restart_after_banked(state.node, token, 0.0);
        }
        if let Some(child) = child_of(current, token) {
            return (HintState { node: child, unbanked: state.unbanked.saturating_add(1) }, self.boost);
        }
        self.fallback(state, token, boundary)
    }

    /// Settles `state` at the end of a piece or recording: banks the longest finished suffix hint, and rolls back any
    /// unfinished prefix tokens before it.
    pub(crate) fn finish(&self, state: HintState) -> f32 {
        if self.is_empty() {
            return 0.0;
        }
        let mut cursor = state.node;
        while cursor != 0 {
            let node = &self.nodes[cursor as usize];
            if node.is_end {
                let kept = state.unbanked.min(node.depth);
                return -f32::from(state.unbanked - kept) * self.boost;
            }
            cursor = node.fail;
        }
        -f32::from(state.unbanked) * self.boost
    }

    fn restart_after_banked(&self, completed: u32, token: usize, prefix_delta: f32) -> (HintState, f32) {
        if let Some(child) = child_of(&self.nodes[completed as usize], token) {
            return (HintState { node: child, unbanked: 1 }, prefix_delta + self.boost);
        }
        if let Some(child) = child_of(&self.nodes[0], token) {
            return (HintState { node: child, unbanked: 1 }, prefix_delta + self.boost);
        }
        (HintState::default(), prefix_delta)
    }

    fn fallback(&self, state: HintState, token: usize, boundary: bool) -> (HintState, f32) {
        let mut cursor = self.nodes[state.node as usize].fail;
        loop {
            let node = &self.nodes[cursor as usize];
            if boundary && node.is_end {
                let kept = state.unbanked.min(node.depth);
                let prefix_delta = -f32::from(state.unbanked - kept) * self.boost;
                return self.restart_after_banked(cursor, token, prefix_delta);
            }
            if let Some(child) = child_of(node, token) {
                let next_unbanked = self.nodes[child as usize].depth.min(state.unbanked.saturating_add(1));
                let diff = i32::from(next_unbanked) - i32::from(state.unbanked);
                #[expect(clippy::cast_precision_loss, reason = "hint depths are under a few dozen tokens")]
                let delta = (diff as f32) * self.boost;
                return (HintState { node: child, unbanked: next_unbanked }, delta);
            }
            if cursor == 0 {
                return (HintState::default(), -f32::from(state.unbanked) * self.boost);
            }
            cursor = node.fail;
        }
    }
}

fn is_control(piece: &str) -> bool {
    piece.is_empty() || (piece.starts_with('<') && piece.ends_with('>'))
}

fn is_word_boundary(piece: &str) -> bool {
    piece.starts_with(WORD_START) || piece.chars().next().is_some_and(|ch| !ch.is_alphanumeric())
}

fn index_pieces(pieces: &[String]) -> HashMap<&str, usize> {
    let mut map = HashMap::with_capacity(pieces.len());
    for (id, piece) in pieces.iter().enumerate() {
        if !is_control(piece) {
            map.entry(piece.as_str()).or_insert(id);
        }
    }
    map
}

/// Gives a lowercase hint its sentence-initial capitalized form as well, since models start sentences in upper case.
fn cased_forms(phrase: &str) -> Vec<String> {
    let prefixed = format!("{WORD_START}{}", phrase.replace(' ', &WORD_START.to_string()));
    let mut forms = vec![prefixed];
    let mut chars = phrase.chars();
    if let Some(first) = chars.next().filter(char::is_ascii_lowercase) {
        let capitalized = format!("{}{}", first.to_ascii_uppercase(), chars.as_str());
        forms.push(format!("{WORD_START}{}", capitalized.replace(' ', &WORD_START.to_string())));
    }
    forms
}

/// Tokenizes `text` into subword IDs by `SentencePiece` BPE merge rank (lowest token ID merges first, breaking ties
/// left-to-right), returning `None` if `text` contains a character absent from the vocabulary.
fn tokenize(text: &str, by_piece: &HashMap<&str, usize>) -> Option<Vec<usize>> {
    let mut parts: Vec<String> = text.chars().map(|ch| ch.to_string()).collect();
    if !parts.iter().all(|part| by_piece.contains_key(part.as_str())) {
        return None;
    }
    while parts.len() > 1 {
        let best = (0..parts.len() - 1)
            .filter_map(|i| {
                let merged = format!("{}{}", parts[i], parts[i + 1]);
                by_piece.get(merged.as_str()).map(|&rank| (rank, i, merged))
            })
            .min_by_key(|&(rank, i, _)| (rank, i));
        let Some((_, at, merged)) = best else { break };
        parts.splice(at..=at + 1, [merged]);
    }
    parts.iter().map(|part| by_piece.get(part.as_str()).copied()).collect()
}

fn child_of(node: &Node, token: usize) -> Option<u32> {
    node.children.binary_search_by_key(&token, |&(t, _)| t).ok().map(|idx| node.children[idx].1)
}

fn insert(nodes: &mut Vec<Node>, tokens: &[usize]) {
    if tokens.is_empty() {
        return;
    }
    let mut current = 0u32;
    for (step, &token) in tokens.iter().enumerate() {
        let Ok(depth) = u16::try_from(step + 1) else { return };
        if let Some(next) = child_of(&nodes[current as usize], token) {
            current = next;
        } else {
            let Ok(next) = u32::try_from(nodes.len()) else { return };
            nodes.push(Node { children: Vec::new(), fail: 0, depth, is_end: false });
            let children = &mut nodes[current as usize].children;
            if let Err(pos) = children.binary_search_by_key(&token, |&(t, _)| t) {
                children.insert(pos, (token, next));
            }
            current = next;
        }
    }
    nodes[current as usize].is_end = true;
}

fn connect_failures(nodes: &mut [Node]) {
    let mut queue = VecDeque::new();
    let root_children = nodes[0].children.clone();
    for (_, child) in root_children {
        nodes[child as usize].fail = 0;
        queue.push_back(child);
    }
    while let Some(u) = queue.pop_front() {
        let children = nodes[u as usize].children.clone();
        let u_fail = nodes[u as usize].fail;
        for (token, v) in children {
            let mut f = u_fail;
            let fail = loop {
                if let Some(target) = child_of(&nodes[f as usize], token) {
                    break target;
                }
                if f == 0 {
                    break 0;
                }
                f = nodes[f as usize].fail;
            };
            nodes[v as usize].fail = fail;
            queue.push_back(v);
        }
    }
}
