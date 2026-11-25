//! What became of what was dictated, in what was sent: sent as it was written, changed, or not there. Text that was typed
//! is what's left over.

use std::cmp::Reverse;

use crate::score::{Span, Word, apostrophes, found, words};

/// What became of one recording's text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Became {
    AsWritten,
    /// Sent as this, whose words are the same, but for capitals and marks.
    Marks(String),
    /// Sent as this, whose words are close to its own.
    Changed(String),
    NotThere,
}

/// What became of each of `written`, in order, in `sent`, which they were all dictated into. Each word sent came from one
/// recording at most. Those there as they were written are found first, then the longest, since it's the surest.
pub(super) fn became(written: &[String], sent: &str) -> Vec<Became> {
    let sent = apostrophes(sent);
    let written: Vec<String> = written.iter().map(|text| apostrophes(text)).collect();
    let sent_words = words(&sent);
    let mut taken: Vec<Span> = Vec::new();
    let mut became: Vec<Option<Became>> = vec![None; written.len()];
    let mut longest: Vec<usize> = (0..written.len()).collect();
    longest.sort_by_key(|&i| Reverse(words(&written[i]).len()));
    for &i in &longest {
        let free = |span: &Span| !taken.iter().any(|taken| taken.start < span.end && span.start < taken.end);
        if let Some(span) = found(&sent, written[i].trim(), true, |_| true).into_iter().find(free) {
            taken.push(span);
            became[i] = Some(Became::AsWritten);
        }
    }
    for &i in &longest {
        if became[i].is_some() {
            continue;
        }
        let Some(span) = closest(&words(&written[i]), &sent_words, &taken) else { continue };
        let sent_as = sent[span.clone()].to_string();
        taken.push(span);
        let same = |a: &[Word], b: &[Word]| a.iter().map(|word| &word.text).eq(b.iter().map(|word| &word.text));
        became[i] = Some(if same(&words(&written[i]), &words(&sent_as)) {
            Became::Marks(sent_as)
        } else {
            Became::Changed(sent_as)
        });
    }
    became.into_iter().map(|became| became.unwrap_or(Became::NotThere)).collect()
}

/// Where the words closest to `written` are among those of `sent` that aren't `taken`, if they're close enough to be its
/// own.
fn closest(written: &[Word], sent: &[Word], taken: &[Span]) -> Option<Span> {
    let is_taken = |word: &Word| taken.iter().any(|taken| taken.start < word.span.end && word.span.start < taken.end);
    let written: Vec<char> = joined(written).into_iter().map(|(c, _)| c).collect();
    let runs = sent.split(is_taken).filter(|run| !run.is_empty());
    let (edits, span) = runs.filter_map(|run| closest_in(&written, run)).min_by_key(|(edits, _)| *edits)?;
    (edits * 5 <= written.len() * 2).then_some(span)
}

/// `words` with a space between, each character with the word it's of.
fn joined(words: &[Word]) -> Vec<(char, usize)> {
    let mut joined = Vec::new();
    for (i, word) in words.iter().enumerate() {
        if i > 0 {
            joined.push((' ', i));
        }
        joined.extend(word.text.chars().map(|c| (c, i)));
    }
    joined
}

/// The whole words of `run` that `written` is the fewest edits from, and how many: characters swapped, left out or put
/// in, so "work trees" is one from "worktrees".
fn closest_in(written: &[char], run: &[Word]) -> Option<(usize, Span)> {
    let sent = joined(run);
    let starts_a_word = |at: usize| at == 0 || sent.get(at - 1).is_some_and(|&(c, _)| c == ' ');
    let ends_a_word = |at: usize| at > 0 && sent.get(at).is_none_or(|&(c, _)| c == ' ');
    // For each place in the run, the fewest edits to end there, and the word it began at. It can begin at any word.
    let mut before: Vec<Option<(usize, usize)>> = (0..=sent.len())
        .map(|at| starts_a_word(at).then(|| (0, sent.get(at).map_or(run.len(), |&(_, w)| w))))
        .collect();
    let more = |found: Option<(usize, usize)>, edits: usize| found.map(|(so_far, began)| (so_far + edits, began));
    for &c in written {
        let mut now = vec![more(before[0], 1)];
        for (at, &(sent_c, _)) in sent.iter().enumerate() {
            let swapped = more(before[at], usize::from(c != sent_c));
            let ways = [swapped, more(before[at + 1], 1), more(now[at], 1)];
            now.push(ways.into_iter().flatten().min_by_key(|&(edits, _)| edits));
        }
        before = now;
    }
    let ends = before.iter().enumerate().filter(|&(at, _)| ends_a_word(at));
    let ended = ends.filter_map(|(at, found)| found.map(|(edits, began)| (edits, began, sent[at - 1].1)));
    let (edits, first, last) = ended.filter(|&(_, first, last)| first <= last).min_by_key(|&(edits, ..)| edits)?;
    Some((edits, run[first].span.start..run[last].span.end))
}

#[cfg(test)]
mod tests {
    use super::{Became, became};

    fn of(written: &[&str], sent: &str) -> Vec<Became> {
        became(&written.iter().map(ToString::to_string).collect::<Vec<_>>(), sent)
    }

    fn changed(sent_as: &str) -> Became {
        Became::Changed(sent_as.to_string())
    }

    #[test]
    fn whats_typed_around_it_is_left_over() {
        assert_eq!(of(&["Fix the test."], "say: Please Fix the test. Then rest"), [Became::AsWritten]);
    }

    #[test]
    fn what_was_changed_is_found_with_the_words_it_was_changed_to() {
        let sent = "Please Look at the worktrees, then rest";
        assert_eq!(of(&["Look at the work trees."], sent), [changed("Look at the worktrees")]);
        assert_eq!(
            of(&["Look at the work trees."], "Look at all the work trees."),
            [changed("Look at all the work trees")]
        );
        assert_eq!(
            of(&["Cargo test it all."], "Run cargo-test it all."),
            [Became::Marks("cargo-test it all".to_string())]
        );
        assert_eq!(of(&["fix the test"], "Fix the test."), [Became::Marks("Fix the test".to_string())]);
        assert_eq!(of(&["The user’s test."], "The user's test."), [Became::AsWritten], "either apostrophe");
        assert_eq!(of(&["The user's test."], "The user’s test."), [Became::AsWritten]);
    }

    #[test]
    fn what_was_deleted_is_not_there() {
        assert_eq!(of(&["Fix the flaky test."], "Look at the worktrees."), [Became::NotThere]);
        assert_eq!(of(&["Fix the flaky test."], ""), [Became::NotThere]);
        assert_eq!(of(&["tests"], "Some contests"), [Became::NotThere], "only the end of a word");
        assert_eq!(of(&["some"], "Something else"), [Became::NotThere], "only the start of one");
    }

    #[test]
    fn each_of_several_is_found_in_words_of_its_own() {
        let sent = "Then run it. Please Fix the test. Fix the tests.";
        assert_eq!(
            of(&["Fix the test.", "Run it now.", "Then run it.", "Fix the test."], sent),
            [Became::AsWritten, Became::NotThere, Became::AsWritten, changed("Fix the tests")]
        );
    }
}
