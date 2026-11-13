//! What the CLI's requests for approval ask of the person. Two of its tools ask something rather than do something:
//! `AskUserQuestion`, whose questions want answers, and `ExitPlanMode`, whose plan wants approving. Every other tool
//! asks to be used. `docs/claude-cli.md` has how the CLI sends each, and how it takes what the person decided, which
//! `answer` writes.

mod answer;

pub(super) use answer::{Waiting, denied_in_its_own_words, response};

mod asking;
pub(super) use asking::asking;
