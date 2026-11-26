use gpui_kit::Hsla;

use crate::named::named;

named! {
    /// How strongly a colour is drawn over another: a role at one of seven steps, from the faintest, so a wash, an edge
    /// or a faded word takes a step rather than a number of its own, and the design has the same seven. A colour faded
    /// for something else, such as the mark's own drawing, or a part faded whole, such as a prompt waiting in the
    /// queue, takes none.
    Strength {
        /// Under the pointer: a borderless button's wash, a menu's row and a list's row.
        XXSoft,
        /// What's chosen in a list, and a wash: a card's, such as a plan's, a banner's, a pill's, a plain badge's, a
        /// composer's that's planning, and a change in an image's comparison.
        XSoft,
        /// Pressed, and a stronger wash: a diff's line, a badge's, a pull request's bar, dictation's, and a limit's
        /// track.
        Soft,
        /// A line on a wash, such as a card's, each match of what's found, and the track of the usage ring and of the
        /// cache's mark.
        Medium,
        /// What's selected in text, the edge of what's washed, such as a label's or a pull request's button's, the
        /// words that changed in a diff's line, a boxed field's fill, a split diff's side with no line, the tab's line
        /// under the pointer, and the stripes over what only one image covers.
        Strong,
        /// A strong edge, such as a comment's in the diff, a composer's that's planning or a change's in an image's
        /// comparison, the match being shown, and counts as they were last read.
        XStrong,
        /// What's faded but still read, such as a sidebar's heading, a call's command, a changed line's number in a
        /// call, and dictation's line.
        XXStrong,
    }
}

impl Strength {
    /// How much of the colour shows, from 0 to 1.
    pub const fn share(self) -> f32 {
        match self {
            Strength::XXSoft => 0.06,
            Strength::XSoft => 0.09,
            Strength::Soft => 0.12,
            Strength::Medium => 0.2,
            Strength::Strong => 0.3,
            Strength::XStrong => 0.55,
            Strength::XXStrong => 0.75,
        }
    }

    /// `color` at this strength, over whatever it's drawn on.
    #[expect(clippy::disallowed_methods, reason = "the one place a role is drawn at a strength")]
    pub fn of(self, color: Hsla) -> Hsla {
        color.opacity(self.share())
    }

    /// `color` at this strength as it fades in or out, `shown` of the way from none of it, at 0, to all, at 1.
    #[expect(clippy::disallowed_methods, reason = "a strength, faded as what's drawn with it fades")]
    pub fn of_fading(self, color: Hsla, shown: f32) -> Hsla {
        color.opacity(self.share() * shown.clamp(0., 1.))
    }

    /// How the spec's table writes it: its name and its share, as a percentage.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 2] {
        [format!("{self:?}"), format!("{}%", (self.share() * 100.).round())]
    }
}
