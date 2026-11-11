//! What every place a prompt is written has, besides its text field and its files: in a chat's composer and in the
//! new-workspace dialog. `Shift-Tab` turns plan mode on and off there, so does the `+` at the bottom left, whose menu
//! also attaches files, and plan mode shows in copper over its texture. A chat's mode is the hub's, and the dialog's is
//! its own until the workspace is made, so each place says whether it's on and handles [`TogglePlanMode`] itself.

use std::path::PathBuf;

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AbsoluteLength, Anchor, App, Context, Div, FocusHandle, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Role, Stateful, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use look::Hand as _;

use crate::TogglePlanMode;
use crate::attached;

/// Where `Shift-Tab` turns plan mode on and off, in place of the text field's own outdent.
pub(crate) const KEY_CONTEXT: &str = "Composer";
const PLAN_LABEL: &str = "Plan mode";
/// What the `+` is called, since it shows only its icon.
const PLUS_LABEL: &str = "Attachments and plan mode";

/// Plan mode where a prompt is written, as the `+` menu's item shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Plan {
    /// There's no chat to change, as while a workspace's first is being made, so the item is greyed out.
    Unavailable,
    Off,
    On,
}

impl Plan {
    pub(crate) fn of(planning: bool) -> Plan {
        if planning { Plan::On } else { Plan::Off }
    }
}

/// `place`, a place where a prompt is written, made into one: `Shift-Tab` and the `+` menu's Plan mode reach `toggle`
/// through it, and while `planning`, plan mode's texture lies under everything put in it after this, inside its
/// corners, whose radius is `corner`. The texture is named, so assistive apps can say what it shows.
pub(crate) fn place<T: 'static>(
    place: Stateful<Div>,
    planning: bool,
    corner: AbsoluteLength,
    cx: &mut Context<T>,
    toggle: fn(&mut T, &mut Context<T>),
) -> Stateful<Div> {
    let texture = div()
        .id("plan-texture")
        .role(Role::Status)
        .aria_label(PLAN_LABEL)
        .absolute()
        .inset_0()
        .child(look::plan_texture(cx, corner))
        .test_support();
    place
        .key_context(KEY_CONTEXT)
        .relative()
        .on_action(cx.listener(move |t, _: &TogglePlanMode, _, cx| toggle(t, cx)))
        .when(planning, |place| place.child(texture))
}

/// The surface and edge of a place where a prompt is written, whose edge is `edge` otherwise: the composer's surface,
/// or in plan mode, both in its copper.
pub(crate) fn colors(planning: bool, edge: Hsla, cx: &App) -> (Hsla, Hsla) {
    let surface = cx.theme().muted;
    if planning { look::plan_colors(surface, cx) } else { (surface, edge) }
}

/// The `+` with ID `id`, and its menu of what goes with a prompt besides its text: files, handed to `attach`, and plan
/// mode, as `plan` says, with its key. Plan mode is asked for from `field`, the text field, which is where `Shift-Tab`
/// is bound, so the item does as the key does and the menu finds the key to show. However the menu closes, focus goes
/// to the field.
pub(crate) fn plus_menu<T, A>(
    id: &'static str,
    plan: Plan,
    field: FocusHandle,
    cx: &mut Context<T>,
    attach: A,
) -> impl IntoElement + use<T, A>
where
    T: 'static,
    A: Fn(&mut T, Vec<PathBuf>, &mut Context<T>) + Clone + 'static,
{
    let this = cx.weak_entity();
    Button::new(id)
        .hand()
        .ghost()
        .xsmall()
        .icon(IconName::Plus)
        .accessibility_label(PLUS_LABEL)
        .tooltip(PLUS_LABEL)
        .dropdown_menu_with_anchor(Anchor::BottomLeft, move |menu, _, _| {
            let (this, attach) = (this.clone(), attach.clone());
            let files = PopupMenuItem::new("Attach files…").icon(Lucide::Paperclip).on_click(move |_, _, cx| {
                let _ = this.update(cx, |_, cx| attached::pick(cx, attach.clone()));
            });
            // The kit draws an item's own icon rather than its check, so the check takes the icon's place.
            let icon = if plan == Plan::On { Icon::new(IconName::Check) } else { Icon::new(Lucide::Map) };
            let planning = PopupMenuItem::new(PLAN_LABEL)
                .icon(icon)
                .checked(plan == Plan::On)
                .disabled(plan == Plan::Unavailable)
                .action(Box::new(TogglePlanMode));
            menu.action_context(field.clone()).item(files).item(planning)
        })
}
