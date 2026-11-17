//! What the chat composer and new-workspace prompt share: their writing area, live dictation, plan mode and the `+`
//! menu. `Shift-Tab` turns plan mode on and off, as its menu item does, and plan mode shows in copper over its texture.
//! A chat's mode is the hub's, and the dialog's is its own until the workspace is made, so each place says whether it's
//! on and handles [`TogglePlanMode`] itself.

use std::path::PathBuf;

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::base::GlobalState;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::{IconName, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Anchor, App, Context, Div, Entity, FocusHandle, Focusable as _, Hsla, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Role, StatefulInteractiveElement as _, Styled, TestSupportExt as _, div,
};
use look::Shaped as _;

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
/// through it, its corners are rounded as `corner`, and while `planning`, plan mode's texture lies under everything
/// put in it after this, inside those corners. The texture is named, so assistive apps can say what it shows.
pub(crate) fn place<T: 'static, E: InteractiveElement + IntoElement + ParentElement + Styled>(
    place: E,
    planning: bool,
    corner: look::Corner,
    cx: &mut Context<T>,
    toggle: fn(&mut T, &mut Context<T>),
) -> E {
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
        .corners(corner)
        .on_action(cx.listener(move |t, _: &TogglePlanMode, _, cx| toggle(t, cx)))
        .when(planning, |place| place.child(texture))
}

/// The surface and edge of a place where a prompt is written, whose edge is `edge` otherwise: the composer's surface,
/// or in plan mode, both in its copper.
pub(crate) fn colors(planning: bool, edge: Hsla, cx: &App) -> (Hsla, Hsla) {
    let surface = look::colors(cx).inset;
    if planning { look::plan_colors(surface, cx) } else { (surface, edge) }
}

/// The prompt and its live words, with one inset and keyboard context. The caller chooses the minimum height;
/// spare room below the current lines still focuses the input rather than selecting text elsewhere in the window.
pub(crate) fn writing_area(
    input: &Entity<TextareaState>,
    textarea: Textarea,
    dictation: &Entity<dictation_ui::Field>,
    cx: &App,
) -> Div {
    let focus = input.focus_handle(cx);
    dictation_ui::Field::keys(
        v_flex()
            .w_full()
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                // The whole writing area belongs to the input; neither selection nor an ancestor takes focus.
                GlobalState::suppress_text_selection(cx);
                window.focus(&focus, cx);
                window.prevent_default();
            })
            .child(div().w_full().px(look::Space::Small).pt(look::Space::Small).child(textarea.appearance(false)))
            .children(dictation.read(cx).live_line(cx).map(|line| {
                div()
                    .w_full()
                    .px(look::Space::Small)
                    .pt(look::Space::XSmall)
                    .child(line.px(gpui_kit::component::Size::default().input_px()))
            })),
        dictation,
    )
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
    look::icon_button(id, look::ButtonKind::Borderless, look::ControlSize::Small, IconName::Plus)
        .accessibility_label(PLUS_LABEL)
        .tooltip(PLUS_LABEL)
        .menu(Anchor::BottomLeft, move |menu, _, _| {
            let (this, attach) = (this.clone(), attach.clone());
            let files = look::menu_row("Attach files…").icon(Lucide::Paperclip).on_choose(move |_, cx| {
                let _ = this.update(cx, |_, cx| attached::pick(cx, attach.clone()));
            });
            let planning = look::menu_row(PLAN_LABEL)
                .icon(Lucide::Map)
                .chosen(plan == Plan::On)
                .off(plan == Plan::Unavailable)
                .key(Box::new(TogglePlanMode));
            menu.keys_from(field.clone()).row(files).row(planning)
        })
}
