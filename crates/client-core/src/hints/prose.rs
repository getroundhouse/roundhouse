//! Pulling code spans, file paths, identifiers, mid-sentence proper nouns and lowercase words English rarely uses out
//! of Markdown prose and commands.

use super::HintCollector;
use super::english::{is_common_word, is_rare_lowercase};

impl HintCollector {
    /// Extracts inline Markdown code spans (`` `…` ``), paths, code identifiers (`CamelCase`, `snake_case`, acronyms),
    /// mid-sentence proper nouns (`Core ML`, `Neural Engine`, `Whisper`, `Parakeet`) and lowercase words English rarely
    /// uses (`tokio`, `kustomize`) from `text`.
    pub(super) fn add_prose(&mut self, text: &str) {
        let mut inside = false;
        for part in text.split('`') {
            if inside {
                self.add_code_span(part.trim());
            }
            inside = !inside;
        }
        let plain = strip_markdown_targets(text);
        for line in plain.lines() {
            self.scan_prose_line(line);
        }
    }

    fn add_code_span(&mut self, span: &str) {
        if span.is_empty() || span.contains('\n') || span.len() > 80 || span.starts_with('-') {
            return;
        }
        if (span.contains('/') || span.contains('\\')) && !span.contains(' ') {
            self.add_path(span);
            return;
        }
        let has_syntax = span.chars().any(|c| matches!(c, '=' | ':' | '(' | ')' | '<' | '>' | '[' | ']' | '&' | ';'));
        if has_syntax {
            for piece in span.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-') {
                if !piece.is_empty() {
                    self.add_term(piece);
                }
            }
        } else if span.split_whitespace().count() <= 3 {
            self.add_term(span);
        }
    }

    fn scan_prose_line(&mut self, line: &str) {
        let raw_tokens: Vec<&str> = line.split_whitespace().collect();
        let mut at_sentence_start = true;
        let mut idx = 0;
        while idx < raw_tokens.len() {
            let raw = raw_tokens[idx];
            let starts_clause = at_sentence_start || starts_with_opener(raw);
            let ends_clause = ends_sentence_or_clause(raw);
            let word = trim_word(raw);
            if word.contains('/') && !word.starts_with("http") && !word.starts_with("file:") && word.len() <= 80 {
                self.add_path(word);
            } else if looks_like_code_identifier(word) {
                self.add_term(word);
            } else if !starts_clause && is_capitalized_word(word) {
                if !ends_clause
                    && !is_common_word(word)
                    && let Some(&next_raw) = raw_tokens.get(idx + 1)
                    && !starts_with_opener(next_raw)
                    && is_capitalized_word(trim_word(next_raw))
                {
                    let second = trim_word(next_raw);
                    self.add_proper_noun(&format!("{word} {second}"));
                    at_sentence_start = ends_sentence_or_clause(next_raw);
                    idx += 2;
                    continue;
                }
                if !is_common_word(word) {
                    self.add_proper_noun(word);
                }
            } else if is_rare_lowercase(word) {
                self.add_term(word);
            }
            at_sentence_start = ends_clause;
            idx += 1;
        }
    }
}

/// Drops `](url)` link targets so `file:///Users/...` or `https://...` addresses aren't scanned as prose paths.
fn strip_markdown_targets(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == ']' && chars.peek() == Some(&'(') {
            out.push(' ');
            chars.next();
            for inner in chars.by_ref() {
                if inner == ')' {
                    break;
                }
            }
        } else if ch != '`' {
            out.push(ch);
        } else {
            out.push(' ');
        }
    }
    out
}

fn trim_word(raw: &str) -> &str {
    let trimmed = raw.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '-' && c != '/');
    trimmed.strip_suffix("'s").or_else(|| trimmed.strip_suffix("’s")).unwrap_or(trimmed)
}

fn starts_with_opener(raw: &str) -> bool {
    raw.starts_with(['(', '[', '"', '\'', '“', '‘', '#', '>', '-', '*', '|'])
        || raw.chars().all(|c| c.is_ascii_digit() || c == '.')
}

fn ends_sentence_or_clause(raw: &str) -> bool {
    let stripped = raw.trim_end_matches(['"', '\'', '”', '’', ')', ']', '*']);
    stripped.ends_with(['.', '?', '!', ':', ';', '—', '–', '|'])
}

fn is_capitalized_word(word: &str) -> bool {
    let mut chars = word.chars();
    let Some(first) = chars.next() else { return false };
    first.is_ascii_uppercase() && word.len() >= 2 && chars.all(|c| c.is_alphanumeric() || c == '-')
}

/// Code names inside unquoted prose: `snake_case` or internal uppercase (`SQLite`, `WorkspacePane`, `GPUI`).
fn looks_like_code_identifier(word: &str) -> bool {
    if word.len() < 2 || word.len() > 40 || word.starts_with('-') || !word.chars().any(char::is_alphabetic) {
        return false;
    }
    if word.contains('_') && !word.starts_with('_') {
        return true;
    }
    word.chars().skip(1).any(char::is_uppercase)
}
