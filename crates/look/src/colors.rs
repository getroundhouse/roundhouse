//! Every colour the app draws with, named by what it's for rather than by its value, so a second theme would be a
//! second set of values for the same names. The app is dark only, so there's one set. The kit's theme is made from
//! these, in `theme.rs`, so the kit's own parts draw with them too. `docs/spec/look.md` has the table of them, which a
//! test holds to these.

use gpui_kit::{App, Global, Hsla, rgba};

/// Declares the roles once, with their dark values as `0xrrggbbaa`, so the struct, the list the spec's table and the
/// sheet are checked against, and the dark theme can't disagree about which roles there are.
macro_rules! colors {
    ($($group:literal { $($(#[doc = $doc:literal])+ $role:ident = $dark:literal,)+ })+) => {
        /// The colours of the theme the app draws with, which [`colors`] gives.
        #[derive(Clone, Debug)]
        pub struct Colors {
            $($($(#[doc = $doc])+ pub $role: Hsla,)+)+
        }

        impl Colors {
            /// The dark theme's, which is the app's.
            pub fn dark() -> Self {
                Self { $($($role: hsla($dark),)+)+ }
            }

            /// Each role, in the order they're declared, which is the spec table's, for the sheet that shows them.
            #[cfg(any(test, feature = "testing"))]
            pub fn roles(&self) -> Vec<NamedColor> {
                vec![$($(NamedColor { group: $group, name: stringify!($role), color: self.$role },)+)+]
            }
        }
    };
}

/// One of the [`Colors`], with its name and the group it's in, such as `pane` in the surfaces.
#[cfg(any(test, feature = "testing"))]
#[derive(Clone, Copy, Debug)]
pub struct NamedColor {
    pub group: &'static str,
    pub name: &'static str,
    pub color: Hsla,
}

#[cfg(any(test, feature = "testing"))]
impl NamedColor {
    /// As the spec's table writes it: `#rrggbb`, or `#rrggbbaa` if it isn't opaque.
    pub fn hex(&self) -> String {
        crate::theme::hex(self.color)
    }
}

colors! {
    "Surfaces" {
        /// The workspace's pane, Home and Settings' pages, and the window's ground.
        pane = 0x1513_12ff,
        /// The sidebars, the one with the workspaces and the one with a workspace's changes.
        sidebar = 0x1a17_15ff,
        /// What's set into a surface: the composer, a prompt waiting in the queue above it, and what a call shows, such
        /// as its output or a code block.
        inset = 0x1e1b_19ff,
        /// Anything raised over the pane, whether it opens over the rest, such as a card, a menu, a tooltip or what a
        /// hover shows, or sits in place, such as a pill, a file's header, the update card, or an open pull request's
        /// bar while there's something to do first.
        raised = 0x2420_1eff,
        /// A prompt's bubble: a wash of the brand's copper over the pane, so what was asked is easy to find when
        /// scrolling past the agent's work.
        prompt = 0x3728_1fff,
        /// The top of the app icon's tile, a step above the pane's colour it fades down to.
        icon_tile_top = 0x211d_1bff,
    }
    "Under the pointer, pressed or chosen" {
        /// A row in a list under the pointer, such as Home's or a file's.
        row_hover = 0x211e_1cff,
        /// A call's line under the pointer, and the compaction's.
        line_hover = 0x1e1b_19ff,
        /// A menu's row under the pointer, a step above the menu, a page in Settings' sidebar, under the pointer or
        /// open, and behind code within a line of text, which the kit draws in the grey of its menus' rows.
        menu_hover = 0x2823_21ff,
        /// Something raised, under the pointer.
        raised_hover = 0x2b27_24ff,
        /// Something raised, pressed, and a row of the sidebar that's selected.
        raised_pressed = 0x322d_2aff,
    }
    "Edges" {
        /// The one thin edge that separates, around a card, along a header or between rows.
        edge = 0x2a26_24ff,
        /// The edge of something to use: the composer, a chip, a key, a field such as Settings' search, a button that
        /// opens something, and the button back down to a chat's end.
        control_edge = 0x3531_30ff,
    }
    "Text" {
        /// What to read.
        text = 0xebe7_e3ff,
        /// What's secondary: details beside the main text, and a name that needs nothing from you.
        text_muted = 0xa39d_97ff,
        /// Quieter than muted: line numbers, meta, hints, and a mark with no pull request.
        text_faint = 0x6f69_64ff,
    }
    "Buttons" {
        /// A button with an edge, such as a dialog's Cancel.
        button = 0x211e_1cff,
        /// The one main action an area has, such as Send.
        main = 0xf2ee_e9ff,
        /// The main action under the pointer.
        main_hover = 0xffff_ffff,
        /// The main action pressed.
        main_pressed = 0xddd8_d2ff,
        /// What's on the main action, and on any other light fill, such as an action in its pull request's colour.
        on_main = 0x1d1a_18ff,
    }
    "The brand" {
        /// The brand's copper: empty states, the app icon, keyboard focus, the caret, the edge of a comment being
        /// written, and, quieter, a composer that's planning. Never a status.
        brand = 0xd08a_5cff,
        /// The brand's copper paled toward the text, which marks the chat showing, as the line under its tab, and,
        /// faint, what's found.
        current = 0xd8c2_b3ff,
        /// A link in text.
        link = 0xdba2_7cff,
        /// A link under the pointer.
        link_hover = 0xe8b8_96ff,
    }
    "What happened" {
        /// The card that needs you, such as an approval.
        needs_you = 0xe9b2_4cff,
        /// A warning, such as setup that failed or one the agent's CLI gave.
        warning = 0xe9b2_4cff,
        /// The agent stopped short: the turntable of its mark, and what the chat says of it, such as a turn that
        /// failed.
        stopped = 0xee6f_64ff,
        /// Something else that went wrong, such as a call that failed or the hub that couldn't be reached.
        error = 0xee6f_64ff,
        /// Lines and counts added to a file.
        added = 0x6cc4_88ff,
        /// Lines and counts removed from a file.
        removed = 0xee6f_64ff,
        /// A file that's changed, and what changed in an image.
        modified = 0xe9b2_4cff,
    }
    // In GitHub's colours on a dark background, which the people Roundhouse is for already know.
    "A pull request" {
        /// Open.
        pr_open = 0x3fb9_50ff,
        /// Merged.
        pr_merged = 0xa371_f7ff,
        /// Closed: muted, so it doesn't read as the agent stopping short.
        pr_closed = 0xb960_5aff,
        /// A draft.
        pr_draft = 0x8b94_9eff,
        /// Waiting on GitHub, such as for a place in the merge queue.
        pr_queued = 0xd299_22ff,
    }
    "The window" {
        /// Over everything behind a dialog.
        dim = 0x0000_0099,
        /// A scrollbar's thumb.
        scroll_thumb = 0xffff_ff1f,
        /// A scrollbar's thumb under the pointer.
        scroll_thumb_hover = 0xffff_ff33,
        /// Windows' close button under the pointer, as Windows draws its own.
        windows_close = 0xc42b_1cff,
        /// The glyph on Windows' close button under the pointer.
        on_windows_close = 0xffff_ffff,
    }
}

impl Global for Colors {}

/// How strongly the brand's copper shows behind what's selected: as strongly as the kit lets a selection show, which
/// it holds its own to.
const SELECTION: f32 = 0.3;
/// How strongly what's found shows behind the text: faint, so the matches read as a trail through it without hiding
/// it, and the match being shown more strongly, so it stands out from the rest.
const FOUND: f32 = 0.22;
const FOUND_HERE: f32 = 0.55;

impl Colors {
    /// Behind what's selected in text: the brand's copper, faint.
    pub fn selection(&self) -> Hsla {
        self.brand.opacity(SELECTION)
    }

    /// Behind each match of what's being found: the colour of the chat showing, faint.
    pub fn found(&self) -> Hsla {
        self.current.opacity(FOUND)
    }

    /// Behind the match being shown, in the brand's copper.
    pub fn found_here(&self) -> Hsla {
        self.brand.opacity(FOUND_HERE)
    }
}

/// The colours the app draws with, which `apply` installs with the theme.
pub fn colors(cx: &App) -> &Colors {
    cx.global::<Colors>()
}

/// A colour from `0xrrggbbaa`, made as the kit makes one from `#rrggbbaa`, a byte at a time, so a role and the kit's
/// theme give exactly the same colour.
fn hsla(value: u32) -> Hsla {
    rgba(value).into()
}

#[cfg(test)]
mod tests;
