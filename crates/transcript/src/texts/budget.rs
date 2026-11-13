//! Which texts keep the blocks they're drawn from. A text's blocks take far more memory than what it says, so only
//! the short texts drawn most recently keep them, and the others keep only what they show, for a copy, and are
//! parsed again when they're next drawn. A long one keeps its blocks, since it's parsed on another thread and would
//! draw nothing meanwhile, and so does one that's hidden, so it shows again at once.

use super::{AT_ONCE, Store, Text, TextKey};

/// How long the short texts whose blocks are kept can come to, in bytes of what they say, but for those drawn in this
/// frame. A text's blocks, with the kit's layout of them once they're drawn, take about 20 bytes for each of its own,
/// so this is about 2.5 MB. `docs/performance.md` has how that was measured.
const BLOCKS_KEPT: usize = 128 * 1024;
/// How many short texts are parsed between each look at whether too many blocks are kept.
const PARSES_A_LOOK: u32 = 8;

impl Store {
    /// A frame has started: puts away blocks, as [`Store::keep_fewer_blocks`] says, if any text was parsed since the
    /// last look.
    pub(super) fn frame_started(&mut self) {
        // Before the frame is counted, so what the one just ended drew isn't put away. A row can draw more short texts
        // than are kept, as a call does with a long subagent's work gathered into it, and it draws them all again in
        // the next frame, which would parse them again, and so on each frame after.
        if self.parses > 0 {
            self.parses = 0;
            self.keep_fewer_blocks();
        }
        self.frame += 1;
    }

    /// Counts a short text parsed, and after every [`PARSES_A_LOOK`] of them, puts away blocks.
    pub(super) fn parsed_one(&mut self) {
        self.parses += 1;
        if self.parses >= PARSES_A_LOOK {
            self.parses = 0;
            self.keep_fewer_blocks();
        }
    }

    /// Puts away the blocks of the short texts drawn longest ago, so those kept come to no more than [`BLOCKS_KEPT`],
    /// but for those drawn in this frame, which its rows hold anyway. A long text keeps its blocks, as parsing it
    /// again on another thread would leave it blank meanwhile.
    fn keep_fewer_blocks(&mut self) {
        let frame = self.frame;
        let mut short: Vec<(u64, TextKey, usize)> = (self.texts.iter())
            .filter_map(|(id, text)| match text {
                Text::Parsed { parsed } if parsed.source.len() <= AT_ONCE => {
                    Some((self.places.last_drawn(id)?, id.clone(), parsed.source.len()))
                }
                Text::Parsed { .. } | Text::Read { .. } | Text::Parsing { .. } | Text::Plain { .. } => None,
            })
            .filter(|(drawn, ..)| *drawn < frame)
            .collect();
        let mut kept: usize = short.iter().map(|(_, _, len)| len).sum();
        short.sort_unstable_by_key(|(drawn, ..)| *drawn);
        for (_, id, len) in short {
            if kept <= BLOCKS_KEPT {
                break;
            }
            if let Some(Text::Parsed { parsed, .. }) = self.texts.remove(&id) {
                self.texts.insert(id, Text::Read { leaves: parsed.leaves.clone() });
            }
            kept -= len;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Text, TextAt, TextKey, Texts};
    use super::{BLOCKS_KEPT, PARSES_A_LOOK};

    fn at(seq: u64) -> TextAt {
        TextAt::only(protocol::model::Seq::new(seq), 0)
    }

    /// Has `texts` draw `count` texts of `words`, each in a frame of its own, and says which then have blocks. They're
    /// drawn from the last in the chat to the first, so going by when they were drawn isn't going by where they are.
    fn drawn_from_the_last(texts: &Texts, words: &str, count: u64) -> Vec<u64> {
        for seq in (0..count).rev() {
            texts.new_frame();
            texts.drawn_at(at(seq), words);
        }
        let texts_now = texts.0.borrow();
        let parsed = |seq: &u64| matches!(texts_now.texts.get(&TextKey::of(at(*seq))), Some(Text::Parsed { .. }));
        (0..count).filter(parsed).collect()
    }

    #[test]
    fn the_blocks_of_the_short_texts_drawn_longest_ago_are_put_away_but_not_what_they_show() {
        let texts = Texts::alone();
        let words = "word ".repeat(200);
        let count = u64::try_from(2 * BLOCKS_KEPT / words.len()).unwrap();

        let with_blocks = drawn_from_the_last(&texts, &words, count);
        texts.new_frame();
        texts.drawn_at(at(count - 1), &words);

        assert!(with_blocks.contains(&0), "the last drawn keeps them");
        assert!(!with_blocks.contains(&(count - 1)), "the first drawn doesn't");
        let most = BLOCKS_KEPT / words.len() + usize::try_from(PARSES_A_LOOK).unwrap();
        assert!(with_blocks.len() <= most, "{} of {count} kept them", with_blocks.len());
        assert!(
            (0..count).all(|seq| texts.leaves_of(&TextKey::of(at(seq))).is_some()),
            "each still says what it shows"
        );
        let again = matches!(texts.0.borrow().texts.get(&TextKey::of(at(count - 1))), Some(Text::Parsed { .. }));
        assert!(again, "and one drawn again is parsed again");
    }

    /// One row can draw more short texts than are kept, as a call does with a long subagent's work gathered into it,
    /// and draws them all again each frame. They're parsed once, not again each frame.
    #[test]
    fn what_one_frame_drew_is_not_put_away_as_the_next_starts() {
        let texts = Texts::alone();
        let words = "word ".repeat(200);
        let count = u64::try_from(2 * BLOCKS_KEPT / words.len()).unwrap();
        // So some are parsed since the last look when the frame ends, which is when the next frame looks.
        assert!(!count.is_multiple_of(u64::from(PARSES_A_LOOK)));
        let draw_all = || {
            texts.new_frame();
            for seq in 0..count {
                texts.drawn_at(at(seq), &words);
            }
        };
        draw_all();

        draw_all();
        let parsed_again = texts.0.borrow().parses;
        texts.new_frame();

        let texts_now = texts.0.borrow();
        assert_eq!(parsed_again, 0, "none was parsed again");
        assert!(texts_now.texts.values().all(|text| matches!(text, Text::Parsed { .. })), "and each keeps its blocks");
    }

    #[test]
    fn nothing_drawn_in_this_frame_is_put_away() {
        let texts = Texts::alone();
        let words = "word ".repeat(200);
        let count = u64::try_from(2 * BLOCKS_KEPT / words.len()).unwrap();

        texts.new_frame();
        for seq in 0..count {
            texts.drawn_at(at(seq), &words);
        }

        let texts_now = texts.0.borrow();
        assert!(texts_now.texts.values().all(|text| matches!(text, Text::Parsed { .. })));
    }
}
