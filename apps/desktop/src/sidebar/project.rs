//! A project's row in the sidebar: its folder and name, and the button for a new workspace in it. Pressing the row
//! collapses the project, which puts its workspaces away under the row, or brings them back, as
//! [`fold`](super::fold) draws, and dragging it moves the project among the others.

use std::f32::consts::FRAC_PI_2;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Div, ElementId, FontWeight, InteractiveElement as _, IntoElement,
    MouseButton, ParentElement, Role, SharedString, StatefulInteractiveElement as _, Styled, TestSupportExt as _, div,
    radians,
};
use look::Hand as _;
use protocol::model::{Repo, RepoId, WorkspaceId};

use super::drag::{Dragged, Dragging};
use super::{REPO_HEIGHT, has_line, icon_box, lists, needs_a_look};
use crate::app::AppView;

/// The row of a project that's `collapsed` or not, whose workspaces are `out` from under it, from 0 to 1. Dragging it
/// moves the project among the others, as [`drag`](super::drag) says, and a press collapses it. A project with no
/// workspaces has nothing to collapse, so a press does nothing. A collapsed project speaks for the workspaces it has
/// put away: its name stands out while one of them needs a look, as theirs would, and it says how many do. It has no
/// mark, since what its agents are doing isn't about the person.
pub(super) fn row(app: &AppView, repo: &Repo, collapsed: bool, out: f32, cx: &mut Context<AppView>) -> AnyElement {
    let toggled = repo.id.clone();
    let group = SharedString::from(format!("project-{}", repo.id));
    let empty = !app.records().workspaces().iter().any(|w| w.repo_id == repo.id && has_line(w));
    let chevron = (!empty).then(|| chevron(collapsed, out, group.clone()));
    let its = app.records().workspaces().iter().filter(|w| w.repo_id == repo.id && lists(w));
    let to_look_at = if collapsed { its.filter(|w| needs_a_look(app, w, cx)).count() } else { 0 };
    let name_color = if to_look_at > 0 { cx.theme().foreground } else { cx.theme().muted_foreground };
    let name = div().min_w_0().truncate().text_sm().font_weight(FontWeight::MEDIUM).text_color(name_color);
    let to_look_at = (to_look_at > 0).then(|| needing_a_look(&repo.id, to_look_at, cx));
    let (dragged, this) = (Dragged(repo.id.clone()), cx.weak_entity());
    h_flex()
        .id(ElementId::Name(group.clone()))
        .group(group)
        // An open hand rather than a pointing one, since the row can be dragged, whatever a press does.
        .grab()
        .on_drag(dragged, move |Dragged(project), grip, _, cx| {
            let dragging = Dragging::held(project.clone(), grip.y);
            let _ = this.update(cx, |app, cx| {
                app.sidebar.dragging = Some(dragging);
                cx.notify();
            });
            cx.new(|_| Dragged(project.clone()))
        })
        .when(!empty, |row| {
            row.role(Role::Button)
                .aria_label(repo.name.clone())
                .aria_expanded(!collapsed)
                .on_click(cx.listener(move |t, _, _, cx| t.toggle_project(&toggled, cx)))
        })
        .w_full()
        .h(REPO_HEIGHT)
        .px_1p5()
        .gap_2()
        .child(icon_box(IconName::Folder, look::faint()))
        .child(h_flex().flex_1().min_w_0().gap_1().child(name.child(repo.name.clone())).children(chevron))
        .children(to_look_at)
        .child(new_workspace(app, repo, cx))
        .test_support()
        .into_any_element()
}

/// The button for a new workspace in the project, at its row's right end.
fn new_workspace(app: &AppView, repo: &Repo, cx: &mut Context<AppView>) -> Button {
    let new_in = repo.id.clone();
    let creating = app.busy.creating_in() == Some(&repo.id);
    Button::new(ElementId::Name(format!("new-in-{}", repo.id).into()))
        .ghost()
        .small()
        .icon(IconName::Plus)
        .text_color(cx.theme().muted_foreground)
        .tooltip("New workspace")
        .loading(creating)
        .hand_while(!creating)
        // A left press stops here, so the row under it isn't pressed too, which would collapse the project, or
        // start dragging it.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(move |t, _, window, cx| t.open_new_workspace(new_in.clone(), window, cx)))
}

/// How many of a collapsed project's workspaces need a look, before its button for a new workspace, at the brightness
/// of a name that needs one.
fn needing_a_look(repo: &RepoId, count: usize, cx: &App) -> impl IntoElement {
    let says = if count == 1 { "1 workspace needs a look".into() } else { format!("{count} workspaces need a look") };
    div()
        .id(ElementId::Name(format!("project-needs-a-look-{repo}").into()))
        .role(Role::Status)
        .aria_label(says)
        .flex_none()
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(cx.theme().foreground)
        .child(count.to_string())
        .test_support()
}

/// A chevron after the project's name, which shows while the pointer is on the project's row, so it's plain that a
/// press collapses it, and for as long as it's collapsed, so it isn't taken for a project with no workspaces. It
/// points down at workspaces that show and right at a collapsed project, and turns with the fold, by how far `out`
/// the workspaces are.
fn chevron(collapsed: bool, out: f32, group: SharedString) -> Div {
    let chevron = Icon::new(IconName::ChevronRight).size_3().text_color(look::faint()).rotate(radians(FRAC_PI_2 * out));
    let chevron = div().flex_none().child(chevron);
    if collapsed { chevron } else { chevron.invisible().group_hover(group, Styled::visible) }
}

impl AppView {
    /// Collapses a project in the sidebar, or brings its workspaces back, as pressing its row does. It's a setting,
    /// so it lasts once the app quits, and the app draws again for when it changes.
    pub(crate) fn toggle_project(&mut self, repo: &RepoId, cx: &mut Context<Self>) {
        self.sidebar.folding.insert(repo.clone());
        crate::settings::change(cx, |settings| {
            if !settings.collapsed_projects.remove(repo) {
                settings.collapsed_projects.insert(repo.clone());
            }
        });
    }

    /// Brings back the workspaces of the project this one is in, if it's collapsed, so a workspace opened from
    /// somewhere else, such as Home, has its row in the sidebar.
    pub(crate) fn show_in_sidebar(&mut self, workspace: &WorkspaceId, cx: &mut Context<Self>) {
        let Some(repo) = self.records().workspace(workspace).map(|w| w.repo_id.clone()) else { return };
        if crate::settings::get(cx).collapsed_projects.contains(&repo) {
            self.toggle_project(&repo, cx);
        }
    }

    /// Whether a project is being dragged, or is on its way to its place from where it was let go.
    #[cfg(feature = "testing")]
    pub fn dragging_a_project(&self) -> bool {
        self.sidebar.dragging.is_some()
    }

    /// How many projects' workspaces are on their way out from under their rows, or away under them.
    #[cfg(feature = "testing")]
    pub fn projects_folding(&self) -> usize {
        self.sidebar.folding.len()
    }

    /// The projects' order as the settings have it, which a drag saves once it's let go.
    #[cfg(feature = "testing")]
    pub fn saved_project_order(cx: &App) -> Vec<RepoId> {
        crate::settings::get(cx).project_order
    }
}
