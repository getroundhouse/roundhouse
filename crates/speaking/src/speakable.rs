//! What's said of a phrase: its words, without the Markdown that marks them up, since a reply is read whole, with only
//! Markdown's marks taken out, as `docs/spec/chats.md` says.
//!
//! It reads each phrase alone, rather than parsing the text with the `markdown` crate, since a phrase is cut as the
//! text comes, often inside emphasis that closes in a phrase not yet come, which a parser would leave as stars to say.
//! So a mark that would close or open emphasis is taken out whether or not its pair is in the phrase.

use crate::phrases::Kind;

/// What to say of `phrase`: for code, its line as it's written; for prose, its words without Markdown's marks; for a
/// fence, nothing. Empty when nothing in it is a letter or a digit, such as a table's rule, which the voice would
/// otherwise make a noise of.
pub(crate) fn speakable(phrase: &str, kind: Kind) -> String {
    let said = match kind {
        Kind::Fence => return String::new(),
        Kind::Code => phrase.split_whitespace().collect::<Vec<_>>().join(" "),
        Kind::Prose => {
            phrase.lines().map(line_without_marks).filter(|line| !line.is_empty()).collect::<Vec<_>>().join(" ")
        }
    };
    if said.chars().any(char::is_alphanumeric) { said } else { String::new() }
}

/// A line of prose without the marks that begin it, such as a heading's, a list's or a quote's, and those inside it.
fn line_without_marks(line: &str) -> String {
    let mut line = line.trim();
    loop {
        let before = line;
        line = line.trim_start_matches('>').trim_start();
        // A heading's marks are followed by a space, so `#2152` is said as it's written.
        let hashes = line.len() - line.trim_start_matches('#').len();
        if (1..=6).contains(&hashes) && line[hashes..].chars().next().is_none_or(char::is_whitespace) {
            line = line[hashes..].trim_start();
        }
        for bullet in ["- ", "* ", "+ "] {
            line = line.strip_prefix(bullet).unwrap_or(line);
        }
        for task in ["[ ] ", "[x] ", "[X] "] {
            line = line.strip_prefix(task).unwrap_or(line);
        }
        let digits = line.len() - line.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        if digits > 0
            && let Some(rest) = line[digits..].strip_prefix(". ").or_else(|| line[digits..].strip_prefix(") "))
        {
            line = rest;
        }
        if line == before {
            break;
        }
    }
    // A table's row is read as its cells, one after another.
    let cells: Vec<&str> = line.split('|').map(str::trim).filter(|cell| !cell.is_empty()).collect();
    let line = if line.contains('|') { cells.join(", ") } else { line.to_string() };
    words(&line)
}

/// `text` without links' targets, images' addresses, HTML's tags, escapes' backslashes, and the marks around
/// emphasis, struck text and code. Code inside backticks is kept as it's written.
fn words(text: &str) -> String {
    let mut said = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        let before = text[..text.len() - rest.len()].chars().next_back();
        if c == '\\'
            && let Some(escaped) = rest[1..].chars().next()
            && escaped.is_ascii_punctuation()
        {
            said.push(escaped);
            rest = &rest[1 + escaped.len_utf8()..];
            continue;
        }
        if c == '`' {
            let ticks = run(rest, '`');
            match rest[ticks..].find(&rest[..ticks]) {
                Some(close) => {
                    said.push_str(&rest[ticks..ticks + close]);
                    rest = &rest[ticks + close + ticks..];
                }
                // Code that goes on into the next phrase: its backticks are taken out, and its words kept.
                None => rest = &rest[ticks..],
            }
            continue;
        }
        if let Some((shown, after)) = link(rest) {
            said.push_str(&words(shown));
            rest = after;
            continue;
        }
        if c == '<'
            && let Some(close) = rest.find('>')
            && rest[1..close].chars().next().is_some_and(|first| first.is_ascii_alphabetic() || first == '/')
            && !rest[..close].contains(' ')
        {
            rest = &rest[close + 1..];
            continue;
        }
        let marks = match c {
            '*' | '_' => run(rest, c),
            '~' if run(rest, c) == 2 => 2,
            _ => 0,
        };
        if marks > 0 && emphasis(c, before, rest[marks..].chars().next()) {
            rest = &rest[marks..];
            continue;
        }
        let kept = if marks > 0 { marks } else { c.len_utf8() };
        said.push_str(&rest[..kept]);
        rest = &rest[kept..];
    }
    said.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// How many of `mark` `text` starts with.
fn run(text: &str, mark: char) -> usize {
    text.len() - text.trim_start_matches(mark).len()
}

/// Whether a run of `mark` between `before` and `after` opens or closes emphasis, as Markdown has it: it's next to a
/// word on one side, so `5 * 3` keeps its star, and an underscore isn't inside a word, so `snake_case` keeps its own.
fn emphasis(mark: char, before: Option<char>, after: Option<char>) -> bool {
    let touches = |c: Option<char>| c.is_some_and(|c| !c.is_whitespace());
    let word = |c: Option<char>| c.is_some_and(char::is_alphanumeric);
    (touches(before) || touches(after)) && !(mark == '_' && word(before) && word(after))
}

/// A link or an image at the start of `text`, `[shown](target)` or `![shown](target)`: what it shows, and what
/// follows it.
fn link(text: &str) -> Option<(&str, &str)> {
    let text = text.strip_prefix('!').unwrap_or(text).strip_prefix('[')?;
    let close = text.find("](")?;
    let target = text[close + 2..].find(')')?;
    Some((&text[..close], &text[close + 2 + target + 1..]))
}
