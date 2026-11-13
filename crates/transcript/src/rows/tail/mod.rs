//! The end of the transcript: text the agent is still writing, the prompts it hasn't read yet, the working line while
//! it works, and the lines for what it has in the background, which `tail/background.rs` draws. What the agent is
//! writing and the prompts are drawn from the kit's blocks, through `crate::texts`, each at the end of the chat in the
//! order they're drawn, so they're selected with the rest. Once it's recorded, each is drawn in the row of its record
//! from what it was parsed as, and a reply parsed from near its end as it went on is parsed whole there, once.

mod background;

mod ending;
pub(super) use ending::{EndOfChat, Waiting, render_tail, waiting};
use ending::{clipped, tail_line};
