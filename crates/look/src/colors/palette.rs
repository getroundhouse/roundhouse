use gpui_kit::{App, Global, Hsla, rgba};

use super::contrast::Contrast;
use super::made::Made::{Grey, Text, Wash};
use super::made::{Recipe, color};
use super::ramp::{Chroma, Lightness, Ramp, Stop};
use super::srgb::Srgb;
use super::strength::Strength;

/// Declares the roles once, with how the dark theme makes each, so the struct, the list the spec's table and the
/// sheet are checked against, and the dark theme can't disagree about which roles there are. A role is a stop on the
/// ramp of greys, text at a contrast on the page, a wash of the brand over it, or a value of its own, as `0xrrggbbaa`.
macro_rules! colors {
    ($($group:literal { $($(#[doc = $doc:literal])+ $role:ident = $made:expr,)+ })+) => {
        /// The colours of the theme the app draws with, which [`colors`](fn@crate::colors) gives.
        #[derive(Clone, Debug, PartialEq)]
        pub struct Colors {
            $($($(#[doc = $doc])+ pub $role: Hsla,)+)+
        }

        impl Colors {
            /// The dark theme's, which is the app's, made from `DARK`.
            pub fn dark() -> Self {
                Self { $($($role: color($made, &DARK),)+)+ }
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

/// The brand's copper, whose hue the greys take, and which a prompt's bubble is washed with.
const BRAND: Srgb = Srgb::from_hex(0xd0_8a5c);
/// The pane's stop, the darkest, so everything else is lighter than the page.
const PAGE: Stop = Stop(0);
/// How much of the brand's copper a prompt's bubble takes over the pane.
const PROMPT: f64 = 0.18;
/// What the dark theme's greys, text and prompt are made from, so tuning the palette is changing one of these: eleven
/// stops on the brand's hue, at a chroma faint enough to read as warm rather than coloured, from near black to the
/// lightest edge, a little over a hundredth of Oklab's lightness apart. The text is past the lightest stop, on the
/// same hue and chroma, and each text's contrast on the page is set where the text is declared.
pub(super) const DARK: Recipe = Recipe {
    greys: Ramp {
        tint: BRAND,
        chroma: Chroma(0.0105),
        darkest: Lightness(0.175),
        lightest: Lightness(0.304),
        stops: 11,
    },
    page: PAGE,
    brand: BRAND,
};

colors! {
    "Surfaces" {
        /// The workspace's pane, Home and Settings' pages, and the window's ground.
        pane = Grey(PAGE),
        /// The sidebars, the one with the workspaces and the one with a workspace's changes.
        sidebar = Grey(Stop(1)),
        /// What's set into a surface: the composer, a prompt waiting in the queue above it, and what a call shows, such
        /// as its output or a code block.
        inset = Grey(Stop(4)),
        /// Anything raised over the pane, whether it opens over the rest, such as a card, a menu, a tooltip or what a
        /// hover shows, or sits in place, such as a pill, a file's header, the update card, or an open pull request's
        /// bar while there's something to do first.
        raised = Grey(Stop(6)),
        /// A prompt's bubble: a wash of the brand's copper over the pane, so what was asked is easy to find when
        /// scrolling past the agent's work.
        prompt = Wash(PROMPT),
        /// The top of the app icon's tile, a step above the pane's colour it fades down to.
        icon_tile_top = 0x211d_1bff,
    }
    "Under the pointer, pressed or chosen" {
        /// The kit's list's row under the pointer. A list's rows are washed, as `look::ListRow` draws them.
        row_hover = Grey(Stop(4)),
        /// A call's line under the pointer, and the compaction's.
        line_hover = Grey(Stop(2)),
        /// Behind code within a line of text, which the kit draws in its accent's grey.
        menu_hover = Grey(Stop(7)),
        /// Something raised, under the pointer.
        raised_hover = Grey(Stop(8)),
        /// Something raised, pressed, such as the hub switcher.
        raised_pressed = Grey(Stop(10)),
    }
    "Edges and the shadow" {
        /// The one thin edge that separates, around a card, along a header or between rows.
        edge = Grey(Stop(8)),
        /// The edge of something to use: the composer, a chip, a key, a field such as Settings' search, a button that
        /// opens something, and the button back down to a chat's end.
        control_edge = Grey(Stop(10)),
        /// Under what opens over the rest, faint, at the strengths `look::raised` gives it.
        shadow = 0x0000_00ff,
    }
    "Text" {
        /// What to read.
        text = Text(Contrast(92.0)),
        /// What's secondary: details beside the main text, and a name that needs nothing from you.
        text_muted = Text(Contrast(60.0)),
        /// Quieter than muted: line numbers, meta, hints, and a mark with no pull request.
        text_faint = Text(Contrast(30.0)),
    }
    "Buttons" {
        /// A button with an edge, such as a dialog's Cancel.
        button = Grey(Stop(4)),
        /// The one main action an area has, such as Send.
        main = 0xf2ee_e9ff,
        /// The main action under the pointer.
        main_hover = 0xffff_ffff,
        /// The main action pressed.
        main_pressed = 0xddd8_d2ff,
        /// What's on the main action, and on any other light fill, such as an action in its pull request's colour.
        on_main = Grey(Stop(2)),
    }
    "The brand" {
        /// The brand's copper: the mark over an empty pane, the app icon, keyboard focus, the caret, the edge of a
        /// comment being written, and, quieter, a composer that's planning. Never a status.
        brand = BRAND,
        /// The brand's copper paled toward the text, which marks the tab showing, as the line under it, and, faint, the
        /// tab the pointer's over and what's found.
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
        /// Something ready, or done, such as dictation's model once it's prepared.
        done = 0x6cc4_88ff,
        /// The agent stopped short: the turntable of its mark, and what the chat says of it, such as a turn that
        /// failed.
        stopped = 0xee6f_64ff,
        /// Something else that went wrong, such as a call that failed, and the mark over a pane that can't show what
        /// it's for, such as with no hub.
        error = 0xee6f_64ff,
        /// Lines and counts added to a file.
        added = 0x6cc4_88ff,
        /// Lines and counts removed from a file.
        removed = 0xee6f_64ff,
        /// A file that's changed, and what changed in an image.
        modified = 0xe9b2_4cff,
    }
    // In GitHub's colours on a dark background, which the people Roundhouse is for already know, but for an open one,
    // which is the app's own green, so the app has one.
    "A pull request" {
        /// Open, in the green of what's added and what's done.
        pr_open = 0x6cc4_88ff,
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
    // Code, wherever it's shown, in Visual Studio's Dark+ as bat ships it: a role for each of its colours but its plain
    // one, which is `text`, so code is as warm as the words around it. `crate::highlight` draws with them.
    "Code" {
        /// A keyword, such as `if`, `return` or `use`.
        code_keyword = 0xc586_c0ff,
        /// A word that declares or is built in, such as `fn`, `let`, `u32` or `true`, a markup tag's name, and a
        /// heading.
        code_storage = 0x569c_d6ff,
        /// A comment.
        code_comment = 0x608b_4eff,
        /// A string.
        code_string = 0xd69d_85ff,
        /// An escape in a string, such as `\n`.
        code_escape = 0xe3bb_abff,
        /// A number, and a constant a library gives.
        code_number = 0xb5ce_a8ff,
        /// Any other constant.
        code_constant = 0xb4ce_a8ff,
        /// A function's name where it's declared or called.
        code_function = 0xdcdc_aaff,
        /// A type's name, and the class one inherits.
        code_type = 0x4ec9_b0ff,
        /// A variable, and a property's name in CSS.
        code_variable = 0x9cdc_feff,
        /// An attribute's name in markup.
        code_attribute = 0x92ca_f4ff,
        /// The tag around a markup tag's name, and a doctype.
        code_tag = 0x8080_80ff,
        /// A quoted value in XML or HTML, and a property's value in CSS.
        code_value = 0xc8c8_c8ff,
        /// A selector in CSS.
        code_selector = 0xd7ba_7dff,
        /// An at-rule in CSS, such as `@media`.
        code_at_rule = 0x87ce_faff,
        /// A preprocessor's line, such as `#include`.
        code_preprocessor = 0x9b9b_9bff,
        /// XML's processing instruction, such as `<?xml ?>`.
        code_processing = 0x6868_5bff,
        /// An operator in JavaScript, which Dark+ draws apart from other languages' operators.
        code_js_operator = 0x6876_87ff,
        /// The word that declares a method, where a language has one.
        code_method_storage = 0x7072_7eff,
    }
}

impl Global for Colors {}

/// How strongly the brand's copper shows behind what's selected: as strongly as the kit lets a selection show, which
/// it holds its own to.
const SELECTION: Strength = Strength::Strong;
/// How strongly what's found shows behind the text: faint, so the matches read as a trail through it without hiding
/// it, and the match being shown more strongly, so it stands out from the rest.
const FOUND: Strength = Strength::Medium;
const FOUND_HERE: Strength = Strength::XStrong;

impl Colors {
    /// Behind what's selected in text: the brand's copper, faint.
    pub fn selection(&self) -> Hsla {
        SELECTION.of(self.brand)
    }

    /// Behind each match of what's being found: the colour of the chat showing, faint.
    pub fn found(&self) -> Hsla {
        FOUND.of(self.current)
    }

    /// Behind the match being shown, in the brand's copper.
    pub fn found_here(&self) -> Hsla {
        FOUND_HERE.of(self.brand)
    }
}

/// The colours the app draws with, which `apply` installs with the theme.
pub fn colors(cx: &App) -> &Colors {
    cx.global::<Colors>()
}

/// A colour from `0xrrggbbaa`, made as the kit makes one from `#rrggbbaa`, a byte at a time, so a role and the kit's
/// theme give exactly the same colour.
pub(super) fn hsla(value: u32) -> Hsla {
    rgba(value).into()
}
