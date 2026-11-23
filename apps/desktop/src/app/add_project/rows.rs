//! A repository the hub's GitHub accounts can see, as a row of the dialog's list.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::{Icon, h_flex};
use gpui_kit::{App, IntoElement, ParentElement as _, Styled as _, div};
use look::{IconSize, IconSizing as _, RowSize, Space, Type, Typeset as _};
use protocol::client::github_repositories::GithubRepositorySeen;

/// `seen`'s row: GitHub's mark, its owner and name, whether it's private, and which accounts can see it, chosen as
/// `chosen` says.
pub(super) fn row(seen: &GithubRepositorySeen, chosen: bool, cx: &App) -> look::ListRow {
    let colors = look::colors(cx);
    let accounts = seen.seen_by.iter().map(protocol::model::GithubUsername::as_str).collect::<Vec<_>>().join(", ");
    let detail = if seen.private { format!("Private · {accounts}") } else { accounts };
    look::list_row(format!("github-repository-{}", seen.id), RowSize::Regular).chosen(chosen).child(
        h_flex()
            .w_full()
            .gap(Space::Small)
            .items_center()
            .child(Icon::new(Lucide::Github).sized(IconSize::Small).text_color(colors.text_muted))
            .child(div().flex_1().min_w_0().truncate().typeset(Type::Body).child(seen.repository.to_string()))
            .child(div().flex_none().typeset(Type::Detail).text_color(colors.text_faint).child(detail))
            .into_any_element(),
    )
}
