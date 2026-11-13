//! What resting the pointer on a workspace's name in its header shows: where its worktree is, with a button that copies
//! the path, which build its agents are on, where they run, and the ports set aside for it. One worker does all of a
//! workspace's work, every chat's included, so this is the workspace's rather than a chat's.

use gpui_kit::component::hover_card::HoverCard;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    Anchor, AnyElement, App, Div, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Role, SharedString,
    Stateful, StatefulInteractiveElement as _, StyleRefinement, Styled as _, div, px,
};
use look::{InRegion as _, Region};
use protocol::model::{PORTS_PER_WORKSPACE, Workspace};

const WIDTH: Pixels = px(380.);
/// As wide as the widest label, so the values line up.
const LABEL: Pixels = px(56.);

/// `name`, the workspace's name in the header, with the card it opens under it, with its left edge on the name's.
/// `hub_version` is the hub's build, which the next worker is, empty when the app hasn't heard it.
pub(super) fn card(name: impl IntoElement + 'static, workspace: &Workspace, hub_version: &str) -> HoverCard {
    let said = Said::of(workspace, hub_version);
    // It holds the button that copies the path, so it waits a moment for the pointer to cross from the name to it.
    look::hover_card_with_a_button("workspace-card-hover")
        .appearance(false)
        .anchor(Anchor::TopLeft)
        // So the name can still give way to the button beside it, and end in an ellipsis.
        .trigger_style(StyleRefinement::default().min_w_0().overflow_hidden())
        .trigger(name)
        .content(move |_, _, cx| said.card(cx))
}

/// What the card says, from the workspace's record and the hub's build. The path, the version and the ports are each
/// left out while they aren't known.
struct Said {
    path: Option<SharedString>,
    version: Option<SharedString>,
    /// The hub's build, while the workspace's worker is another, which the hub moves it to once nothing in it is at
    /// work.
    moving_to: Option<SharedString>,
    runs_on: SharedString,
    ports: Option<SharedString>,
}

impl Said {
    fn of(workspace: &Workspace, hub_version: &str) -> Self {
        let hub = (!hub_version.is_empty()).then(|| SharedString::from(hub_version.to_string()));
        let worker = workspace.worker_build.as_deref().map(|build| SharedString::from(build.to_string()));
        let moving_to = hub.clone().filter(|hub| worker.as_ref().is_some_and(|worker| worker != hub));
        // A workspace from before each had its own ports has none.
        let ports = (workspace.port_base > 0).then(|| {
            let last = workspace.port_base.saturating_add(PORTS_PER_WORKSPACE - 1);
            format!("{}–{last}", workspace.port_base).into()
        });
        Self {
            path: workspace.path.clone().map(SharedString::from),
            version: worker.or(hub),
            moving_to,
            runs_on: runs_on(workspace),
            ports,
        }
    }

    /// Raised like the app's other cards: the path and the button that copies it, then under a rule, the rest.
    fn card(&self, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let colors = look::colors(cx);
        let path = self.path.clone().map(|path| {
            let mono = theme.mono_font_family.clone();
            // Cut at its start, if it's too long, since its end is the workspace's own folder.
            let text = div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis_start();
            let text = text.font_family(mono).text_xs().child(path.clone());
            let copy = look::copy_button("copy-workspace-path", path, "Copy path");
            let row = h_flex().gap_2().text_color(colors.text_muted).child(text).child(copy);
            v_flex().child(row).child(div().my_2().h(px(1.)).bg(colors.edge))
        });
        let version = self.version.clone().map(|version| {
            let moving = self.moving_to.clone().map(|to| {
                div().text_color(colors.text_faint).child(format!("Moves to {to} once nothing in it is at work"))
            });
            let said = match &self.moving_to {
                Some(to) => format!("Version {version}, moves to {to} once nothing in it is at work"),
                None => format!("Version {version}"),
            };
            row("Version", said, v_flex().child(version).children(moving), cx)
        });
        let ports = self.ports.clone().map(|ports| row("Ports", format!("Ports {ports}"), div().child(ports), cx));
        let rows = v_flex()
            .gap_1()
            .text_xs()
            .text_color(colors.text_muted)
            .children(version)
            .child(row("Runs on", format!("Runs on {}", self.runs_on), div().child(self.runs_on.clone()), cx))
            .children(ports);
        let card = look::raised(cx).region(Region::WorkspaceCard).flex().flex_col().w(WIDTH).px_3().py_2p5();
        card.children(path).child(rows).into_any_element()
    }
}

/// A label, faint, and what it says beside it, read out as `said`, since a screen reader would read the two apart.
fn row(label: &'static str, said: String, value: Div, cx: &App) -> Stateful<Div> {
    let row = h_flex().id(label).role(Role::Label).aria_label(said).items_start().gap_3();
    row.child(div().flex_none().w(LABEL).text_color(look::colors(cx).text_faint).child(label))
        .child(value.flex_1().min_w_0())
}

/// Where the workspace's worker runs. Every hub the app reaches is on this machine, so the local provider's workers
/// are too.
fn runs_on(workspace: &Workspace) -> SharedString {
    if !workspace.provider.is_local() {
        return workspace.provider.to_string().into();
    }
    if cfg!(target_os = "macos") { "This Mac".into() } else { "This computer".into() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aspen() -> Workspace {
        Workspace { path: Some("/workspaces/aspen".into()), port_base: 4000, ..protocol::fixtures::workspace("w") }
    }

    #[test]
    fn a_workspace_on_the_hub_s_build_says_only_that() {
        let said = Said::of(&aspen(), "1.0.0");

        assert_eq!(said.version.as_deref(), Some("1.0.0"));
        assert_eq!(said.moving_to, None);
        assert_eq!(said.ports.as_deref(), Some("4000–4009"));
        assert_eq!(said.path.as_deref(), Some("/workspaces/aspen"));
    }

    #[test]
    fn a_workspace_still_on_another_build_says_which_it_moves_to() {
        let workspace = Workspace { worker_build: Some("0.9.0".into()), ..aspen() };

        let said = Said::of(&workspace, "1.0.0");

        assert_eq!((said.version.as_deref(), said.moving_to.as_deref()), (Some("0.9.0"), Some("1.0.0")));
    }

    #[test]
    fn what_isn_t_known_is_left_out() {
        let workspace = Workspace { path: None, port_base: 0, ..aspen() };

        let said = Said::of(&workspace, "");

        assert_eq!((said.path, said.version, said.ports), (None, None, None));
    }
}
