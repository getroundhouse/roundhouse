//! Where a text is in the chat, which orders it among the others: what draws it, which of its blocks, and where it is
//! among the texts that block draws.

use protocol::model::Seq;

use crate::find::Part;

/// Where a text is in the chat: what draws it, which of its blocks it's drawn in, and where it is among the texts that
/// block draws. Ordered as the chat is, since a row draws its blocks in order, and a block its texts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct TextAt {
    pub(crate) of: Of,
    pub(crate) block: usize,
    pub(crate) among: Among,
}

impl TextAt {
    /// The only text the `block`th block of the event at `seq` draws, such as a reply's.
    pub(crate) fn only(seq: Seq, block: usize) -> Self {
        Self { of: Of::Event(seq), block, among: Among::Only }
    }

    /// The first place in what `of` draws, which is the only text's when it draws one, as the subagent tab's head and
    /// foot do.
    pub(crate) fn first_of(of: Of) -> Self {
        Self { of, block: 0, among: Among::Only }
    }

    /// The `n`th text the end of the chat draws, in the order it draws them: what the agent is still writing, then the
    /// prompts it hasn't read yet.
    pub(crate) fn at_the_end(n: usize) -> Self {
        Self { of: Of::Tail, block: n, among: Among::Only }
    }

    /// The event its images are found by, if it's in one's row.
    pub(crate) fn seq(self) -> Option<Seq> {
        match self.of {
            Of::Event(seq) => Some(seq),
            Of::Head | Of::Tail | Of::Foot => None,
        }
    }
}

/// What draws a text, in the chat's order: the subagent tab's head, which says what its subagent was asked above the
/// tab's first row, an event's row, the end of the chat, with what the agent is still writing and the prompts it
/// hasn't read yet, or the tab's foot, its report, below that.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Of {
    Head,
    Event(Seq),
    Tail,
    Foot,
}

impl Of {
    /// Whether a text drawn here can go on from what it says, as what the agent is still writing does at the end of
    /// the chat. What's drawn anywhere else is recorded, and says all it will, but for what a working subagent has
    /// written lately, in its call's row, which the hub cuts short, so it's parsed whole each time it says more.
    pub(crate) fn can_go_on(self) -> bool {
        match self {
            Of::Tail => true,
            Of::Head | Of::Event(_) | Of::Foot => false,
        }
    }
}

/// Where a text is among those one block draws, in the order the block shows them. Ordered as declared, so `Only` is
/// the first place in a block, which [`TextAt::first_of`] counts on, as the first place in what draws it. `rows::text`
/// says which each text is.
///
/// A call's own texts are placed by what they are, and what it nests by the order it's drawn in, since that work can
/// nest another subagent's, whose events fall between its own in the log, so only the order drawn is the order read.
/// A nested text's place changes as the work before it shows more or less, and it has none while it isn't drawn. A
/// text is kept, and named by a selection's ends, by what it's known by, so its place only orders it and says it's
/// shown, and what's selected of one that's closed is selected again once it's opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Among {
    /// The only text the block draws, such as a reply's.
    Only,
    /// One of a call's own texts that it shows before what it nests: what went wrong, under its line, and what a
    /// subagent was asked.
    Before(Part),
    /// The `n`th text of what a call nests, such as a subagent's words in its call's row, as `Texts::draw` counts them
    /// for a text at [`Place::NextNested`]. A row makes its nested texts in the order it shows them, which a debug
    /// build checks where they're painted, in `crate::selection`. One that isn't drawn has no place.
    Nested(usize),
    /// One of a call's own texts that it shows after what it nests, such as a subagent's report, in the order of their
    /// parts.
    After(Part),
}

/// Where a text to draw goes: at a place, or as the next text the `block`th block of the event at `seq` nests, whose
/// place the texts drawn before it decide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Place {
    At(TextAt),
    NextNested { seq: Seq, block: usize },
}

impl Place {
    /// The event its images are found by, if it's in one's row.
    pub(crate) fn seq(self) -> Option<Seq> {
        match self {
            Place::At(at) => at.seq(),
            Place::NextNested { seq, .. } => Some(seq),
        }
    }
}

impl From<TextAt> for Place {
    fn from(at: TextAt) -> Self {
        Place::At(at)
    }
}
