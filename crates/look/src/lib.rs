//! How the app looks, everywhere it's drawn: every colour it draws with, every kind of text it sets and every corner it
//! rounds, by what each is for, every step of space, every size a button, a bar, a row and an icon come in, the width
//! of an edge, the theme over the kit's made from them, code's colours, the words that changed within a diff's lines,
//! text of different sizes on one baseline, the status mark, the ring for a background task and the robot for a
//! subagent at work there, how an image is fitted to the room there is, the app icon, plan mode's texture, which is
//! made from the mark, the pointing hand, the button that copies, the parts of the window tests find by where they're
//! drawn, how long things take to move, a hover card that waits like a tooltip, a chat's cache, how a count of tokens
//! reads, and the time text on screen reads, which pictures hold still with everything that moves.
//! `docs/spec/look.md` has the rules these follow, and `docs/spec/components.md` the status mark's and the app icon's.

#![warn(clippy::wildcard_enum_match_arm)]

mod app_icon;
mod badges;
mod baseline;
mod cache;
mod chips;
mod clock;
mod colors;
mod control_sizes;
mod controls;
mod copy;
mod counts;
mod dialogs;
mod fan;
mod fields;
mod file_icons;
mod fit;
mod hand;
mod heights;
mod highlight;
mod hover;
mod icon_sizes;
mod keys;
mod mark;
mod menus;
pub mod motion;
mod named;
mod pills;
mod plan;
mod raised;
mod region;
mod robot;
mod shape;
mod space;
#[cfg(test)]
mod spec_table;
mod switches;
mod task_ring;
mod theme;
mod toggles;
mod tokens;
mod tooltips;
mod typography;
mod word_changes;
mod work;

pub use app_icon::{AppIconSize, AppIconStyle, app_icon};
pub use badges::{Badge, BadgeKind, Label, badge, label};
pub use baseline::{LineText, Placed};
pub use cache::{CacheMark, cache_mark, expires_in, minutes_left, next_minute_in_ms};
pub use chips::{Chip, ChipKind, chip};
#[cfg(feature = "testing")]
pub use clock::hold_still;
pub use clock::{
    date, date_and_time, day_and_time, days_apart, local_time, months_apart, next_day_at_ms, now_ms, time_of_day,
};
#[cfg(feature = "testing")]
pub use colors::NamedColor;
pub use colors::{Colors, colors};
pub use control_sizes::ControlSize;
pub use controls::{Button, ButtonKind, MenuButton, Side, Sizing, button, glyph_button, icon_button};
pub use copy::copy_button;
pub use counts::{Count, CountKind, LinesChanged, count, lines_changed};
#[cfg(any(test, feature = "testing"))]
pub use dialogs::shown_dialog;
pub use dialogs::{
    Dialog, DialogWidth, PlainDialog, close_dialog, close_every_dialog, dialog, dialog_open, open_dialog,
    open_plain_dialog, plain_dialog,
};
pub use fields::{Field, FieldKind, field};
pub use file_icons::{FILE_ICON, file_icon, file_icon_asset, folder_icon};
pub use fit::{fit, pixels};
pub use hand::{Hand, Off};
pub use heights::{BarSize, RowSize};
pub use highlight::{Spans, highlight, language, preload_highlighting, try_highlight};
pub use hover::{HoldOpen, HoverCard, hover_card, hover_card_with_a_button};
pub use icon_sizes::{IconSize, IconSizing};
pub use keys::{Key, KeyKind, key};
pub use mark::{Signal, Size, brand, chat_mark, mark};
#[cfg(any(test, feature = "testing"))]
pub use menus::shown_menu;
pub use menus::{Menu, MenuRow, NoteKind, OpenMenu, RightClickMenu, menu, menu_row};
pub use pills::{Pill, PillKind, pill};
pub use plan::{plan_colors, plan_texture};
pub use raised::raised;
pub use region::{InRegion, Region};
pub use robot::robot_at_work;
pub use shape::{Corner, EDGE, QUOTE_RULE, Shaped, TAB_LINE};
pub use space::Space;
pub use switches::{Switch, SwitchKind, switch};
pub use task_ring::{TaskRing, task_ring};
pub use theme::apply;
pub use toggles::{Toggle, toggle};
pub use tokens::tokens;
#[cfg(any(test, feature = "testing"))]
pub use tooltips::shown_tooltip;
pub use tooltips::tooltip;
pub use typography::{Type, Typeset};
pub use word_changes::{LineChange, word_changes};
pub use work::Work;
