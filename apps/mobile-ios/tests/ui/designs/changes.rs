//! What a workspace changed, and its files: the list of changed files with the pull request's bar, a file's diff with
//! the comments left on it for the agent, every file in the checkout, and one of them opened.

#![cfg(test)]

use gpui_kit::assets::IconName;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{AnyElement, App, IntoElement, ParentElement as _, Styled as _, div, px};
use look::{
    BarKind, BarSize, ButtonKind, ControlSize, Corner, IconSize, Lined as _, PillKind, RowSize, Shaped as _, Space,
    SwitchKind, Type, Typeset as _, Work,
};

use super::canvas::Canvas;
use super::parts::{self, Stands};

pub(super) fn draw(canvas: &mut Canvas) {
    canvas.draw("changes", |_, cx| changes(cx));
    canvas.draw("pull-request-states", |_, cx| pull_request_states(cx));
    canvas.draw("diff", |_, cx| diff(None, cx));
    canvas.draw("comment-sheet", |_, cx| diff(Some(comment_sheet(cx)), cx));
    canvas.draw("all-files", |_, cx| all_files(cx));
    canvas.draw("file", |_, cx| file(cx));
}

const BRANCH: &str = "sidebar-pr-state";

/// A file the workspace changed, as its row shows it.
struct Changed {
    id: &'static str,
    folder: &'static str,
    name: &'static str,
    added: usize,
    removed: usize,
    /// How many comments wait on it for the agent.
    comments: usize,
    /// It's new, rather than changed.
    new: bool,
}

const fn changed(id: &'static str, folder: &'static str, name: &'static str, added: usize, removed: usize) -> Changed {
    Changed { id, folder, name, added, removed, comments: 0, new: false }
}

const CHANGED: &[Changed] = &[
    changed("file-1", "apps/desktop/src", "sidebar.rs", 2, 0),
    Changed { new: true, ..changed("file-2", "apps/desktop/tests/ui", "sidebar.rs", 19, 0) },
    changed("file-3", "crates/hub/src", "hub.rs", 2, 0),
    Changed { new: true, ..changed("file-4", "crates/hub/src/hub", "pull_requests.rs", 27, 0) },
    changed("file-5", "crates/protocol/src", "client.rs", 1, 0),
    Changed { comments: 1, ..changed("file-6", "crates/protocol/src", "model.rs", 3, 0) },
    changed("file-7", "crates/worker/src", "git.rs", 6, 4),
    changed("file-8", "docs/spec", "sidebar.md", 1, 1),
];

fn changed_row(file: &Changed, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let comments = (file.comments > 0).then(|| {
        h_flex()
            .flex_none()
            .gap(Space::XSmall)
            .typeset(Type::Detail)
            .text_color(colors.text_muted)
            .child(parts::icon(IconName::MessageSquare, IconSize::Regular, colors.text_muted))
            .child(file.comments.to_string())
    });
    let standing = if file.new {
        parts::icon(IconName::SquarePlus, IconSize::Regular, colors.added)
    } else {
        parts::icon(IconName::SquareDot, IconSize::Regular, colors.modified)
    };
    look::list_row(file.id, RowSize::Large)
        .child(look::file_icon(file.name, cx))
        .child(parts::names(file.name, file.folder, cx))
        .children(comments)
        .child(div().flex_none().child(look::lines_changed(file.added, file.removed)))
        .child(standing)
        .into_any_element()
}

/// The switch between what changed and every file, with the lines changed in all at its end.
fn showing(changes: bool) -> impl IntoElement {
    let switch = look::switch(SwitchKind::Tracked).words("changes", "Changes 8", changes, |_, _| {}).words(
        "all-files",
        "All files",
        !changes,
        |_, _| {},
    );
    let total = changes.then(|| look::lines_changed(61, 5));
    h_flex().px(Space::Medium).pt(Space::Medium).pb(Space::Small).child(switch).child(div().flex_1()).children(total)
}

fn bar(title: &'static str, under: &'static str) -> look::TopBar {
    parts::bar(title).under(under).start(parts::back())
}

/// The comments that wait for the agent, with the button that puts them in the chat.
fn waiting_comments(cx: &App) -> impl IntoElement {
    let colors = look::colors(cx);
    h_flex()
        .flex_none()
        .gap(Space::Small)
        .px(Space::Medium)
        .py(Space::Small)
        .bg(colors.raised)
        .line_over(cx)
        .child(parts::icon(IconName::MessageSquare, IconSize::Large, colors.text_muted))
        .child(div().flex_1().min_w_0().truncate().child("1 comment for the agent"))
        .child(look::button("add-to-chat", ButtonKind::Main, ControlSize::Regular, "Add to chat"))
}

fn changes(cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    parts::screen(cx)
        .child(bar("Changes", BRANCH).end(parts::tap("more", IconName::Ellipsis)))
        .child(parts::pull_request(Some("#182"), Work::Ready, "Ready to merge", Some("Merge"), cx))
        .child(showing(true))
        .child(parts::rows().flex_1().children(CHANGED.iter().map(|file| changed_row(file, cx))))
        .child(waiting_comments(cx))
        .child(parts::foot().bg(colors.raised))
        .into_any_element()
}

/// Every way a pull request's bar stands, with what it offers to do next, as the desktop's bar says them.
fn pull_request_states(cx: &App) -> AnyElement {
    let muted = look::colors(cx).text_muted;
    let state = |says: &'static str, bar: AnyElement| {
        v_flex()
            .child(
                div()
                    .px(Space::Medium)
                    .pt(Space::Medium)
                    .pb(Space::XSmall)
                    .typeset(Type::Detail)
                    .text_color(muted)
                    .child(says),
            )
            .child(bar)
    };
    let number = Some("#182");
    parts::screen(cx)
        .child(parts::bar("A pull request's bar"))
        .child(state(
            "Nothing opened yet",
            parts::pull_request(None, Work::NoPr, "No pull request", Some("Create PR"), cx),
        ))
        .child(state(
            "Work to commit",
            parts::pull_request(number, Work::Open, "3 files uncommitted", Some("Commit"), cx),
        ))
        .child(state("A draft", parts::pull_request(number, Work::Draft, "Draft", Some("Ready for review"), cx)))
        .child(state("Checks running", parts::pull_request(number, Work::Open, "2 of 5 checks running", None, cx)))
        .child(state(
            "A check failed",
            parts::pull_request(number, Work::Open, "1 check failed", Some("Fix checks"), cx),
        ))
        .child(state("Ready", parts::pull_request(number, Work::Ready, "Ready to merge", Some("Merge"), cx)))
        .child(state("Queued", parts::pull_request(number, Work::Queued, "In the merge queue", None, cx)))
        .child(state("Merged", parts::pull_request(number, Work::Merged, "Merged", Some("Archive"), cx)))
        .child(state("Closed", parts::pull_request(number, Work::Closed, "Closed", Some("Archive"), cx)))
        .into_any_element()
}

const DIFFED: &str = "crates/protocol/src/model.rs";

/// Lines the diff leaves out, which a tap brings back.
fn unchanged(id: &'static str, how_many: usize, up: bool, cx: &App) -> impl IntoElement {
    let colors = look::colors(cx);
    let chevron = if up { IconName::ChevronUp } else { IconName::ChevronDown };
    look::list_row(id, RowSize::Small)
        .child(div().w(px(38.)).flex_none().flex().justify_end().child(parts::icon(
            chevron,
            IconSize::Regular,
            colors.text_muted,
        )))
        .child(
            div()
                .pl(Space::Small)
                .typeset(Type::Detail)
                .text_color(colors.text_muted)
                .child(format!("{how_many} unchanged lines")),
        )
}

/// A comment left on a line for the agent, under the line it's about.
fn comment(cx: &App) -> impl IntoElement {
    let colors = look::colors(cx);
    let quiet = |id: &'static str, glyph: IconName, words: &'static str| {
        look::button(id, ButtonKind::Borderless, ControlSize::Small, words).icon(glyph)
    };
    v_flex()
        .flex_none()
        .gap(Space::Small)
        .px(Space::Medium)
        .py(Space::Medium)
        .bg(colors.raised)
        .line_under(cx)
        .line_over(cx)
        .child("Should this say when it was last checked, so a stale state can show as stale?")
        .child(
            h_flex()
                .gap(Space::XSmall)
                .child(div().flex_1())
                .child(quiet("edit", IconName::Pencil, "Edit"))
                .child(quiet("delete", IconName::Trash, "Delete"))
                .child(
                    look::button("add", ButtonKind::Outlined, ControlSize::Small, "Add to chat")
                        .icon(IconName::MessageSquarePlus),
                ),
        )
}

fn diff(over: Option<AnyElement>, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let line = |number, stands, code| parts::code_line(number, stands, code, DIFFED, cx);
    let lines = v_flex()
        .flex_1()
        .min_h_0()
        .overflow_hidden()
        .child(unchanged("above", 31, true, cx))
        .child(line(32, Stands::Same, "    pub setup: SetupStatus,"))
        .child(line(33, Stands::Same, "    /// The first of ten ports set aside for this workspace."))
        .child(line(34, Stands::Same, "    pub port_base: u16,"))
        .child(line(35, Stands::Added, "    /// The pull request for its branch, as the hub last saw it."))
        .child(line(36, Stands::Added, "    #[serde(default)]"))
        .child(line(37, Stands::Added, "    pub pull_request: Option<PullRequest>,"))
        .child(comment(cx))
        .child(line(38, Stands::Same, "}"))
        .child(line(39, Stands::Same, ""))
        .child(line(40, Stands::Same, "#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]"))
        .child(unchanged("between", 8, false, cx))
        .child(line(49, Stands::Same, "impl Workspace {"))
        .child(line(50, Stands::Removed, "    pub fn is_open(&self) -> bool {"))
        .child(line(50, Stands::Added, "    pub fn has_open_work(&self) -> bool {"))
        .child(line(51, Stands::Same, "        self.archived_at_ms.is_none()"))
        .child(line(52, Stands::Same, "    }"))
        .child(unchanged("below", 112, false, cx));
    // Which file of the changed ones this is, with the ones before and after it a tap away.
    let between = look::bar(BarKind::Words, BarSize::Regular)
        .child(parts::tap("previous", IconName::ChevronLeft))
        .child(
            div()
                .flex_1()
                .text_center()
                .typeset(Type::Detail)
                .text_color(colors.text_muted)
                .child("6 of 8 changed files"),
        )
        .child(parts::tap("next", IconName::ChevronRight));
    parts::screen(cx)
        .child(bar("model.rs", "crates/protocol/src").end(parts::tap("more", IconName::Ellipsis)))
        .child(lines)
        .child(div().flex_none().line_over(cx).child(between))
        .child(parts::foot())
        .children(over)
        .into_any_element()
}

/// What tapping a line opens: a comment on it for the agent.
fn comment_sheet(cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let quoted = look::pill(PillKind::Plain, "/// The pull request for its branch, as the hub last saw it.");
    let written = div()
        .w_full()
        .min_h(px(88.))
        .px(Space::Medium)
        .py(Space::Small)
        .corners(Corner::Control)
        .border(look::EDGE)
        .border_color(colors.brand)
        .bg(colors.inset)
        .text_color(colors.text_faint)
        .child("Add a comment for the agent");
    let answers = h_flex()
        .gap(Space::Small)
        .child(look::icon_button("dictate", ButtonKind::Borderless, ControlSize::Regular, IconName::Mic))
        .child(div().flex_1())
        .child(look::button("cancel", ButtonKind::Borderless, ControlSize::Regular, "Cancel"))
        .child(look::button("comment", ButtonKind::Main, ControlSize::Regular, "Comment").off(true));
    look::sheet()
        .title("Comment on line 35")
        .clear(parts::HOME_INDICATOR)
        .child(v_flex().gap(Space::Medium).p(Space::Medium).child(quoted).child(written).child(answers))
        .into_any_element()
}

/// A folder or a file in the checkout, as deep as `depth` folders.
fn entry(id: &'static str, depth: usize, name: &'static str, open: Option<bool>, cx: &App) -> look::ListRow {
    let glyph = match open {
        Some(open) => look::folder_icon(open, cx).into_any_element(),
        None => look::file_icon(name, cx),
    };
    #[expect(clippy::cast_precision_loss, reason = "a few folders deep")]
    let indent = px(20. * depth as f32);
    look::list_row(id, RowSize::Regular)
        .indent(indent)
        .child(glyph)
        .child(div().flex_1().min_w_0().truncate().child(name))
}

fn all_files(cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let modified = || parts::icon(IconName::SquareDot, IconSize::Regular, colors.modified);
    let files = parts::rows()
        .flex_1()
        .child(entry("apps", 0, "apps", Some(false), cx))
        .child(entry("crates", 0, "crates", Some(true), cx))
        .child(entry("hub", 1, "hub", Some(false), cx))
        .child(entry("protocol", 1, "protocol", Some(true), cx))
        .child(entry("protocol-src", 2, "src", Some(true), cx))
        .child(entry("client", 3, "client.rs", None, cx).child(modified()))
        .child(entry("event", 3, "event.rs", None, cx))
        .child(entry("lib", 3, "lib.rs", None, cx))
        .child(entry("model", 3, "model.rs", None, cx).child(modified()))
        .child(entry("cargo", 2, "Cargo.toml", None, cx))
        .child(entry("worker", 1, "worker", Some(false), cx))
        .child(entry("docs", 0, "docs", Some(false), cx))
        .child(entry("agents-md", 0, "AGENTS.md", None, cx))
        .child(entry("cargo-toml", 0, "Cargo.toml", None, cx))
        .child(entry("readme", 0, "README.md", None, cx));
    parts::screen(cx)
        .child(bar("Files", BRANCH).end(parts::tap("more", IconName::Ellipsis)))
        .child(parts::pull_request(Some("#182"), Work::Ready, "Ready to merge", Some("Merge"), cx))
        .child(showing(false))
        .child(div().px(Space::Medium).pb(Space::Small).child(parts::field(
            Some(IconName::Search),
            None,
            "Find a file",
            cx,
        )))
        .child(files)
        .child(parts::foot())
        .into_any_element()
}

const OPENED: &str = "crates/hub/src/hub/pull_requests.rs";

const CODE: &[&str] = &[
    "//! What the hub knows of each workspace's pull request.",
    "",
    "use protocol::model::{PullRequest, WorkspaceId};",
    "",
    "use crate::github::Github;",
    "use crate::hub::Hub;",
    "",
    "impl Hub {",
    "    /// Asks GitHub where the pull request for a workspace's",
    "    /// branch stands, and keeps the answer on its record.",
    "    pub fn refresh_pull_request(&mut self, id: &WorkspaceId) {",
    "        let Some(workspace) = self.workspaces.get(id) else {",
    "            return;",
    "        };",
    "        let found = self.github.pull_request(&workspace.branch);",
    "        if workspace.pull_request != found {",
    "            self.set_pull_request(id, found);",
    "        }",
    "    }",
    "",
    "    fn set_pull_request(",
    "        &mut self,",
    "        id: &WorkspaceId,",
    "        found: Option<PullRequest>,",
    "    ) {",
    "        self.workspaces.update(id, |workspace| {",
    "            workspace.pull_request = found;",
    "        });",
    "        self.feed.changed(id);",
    "    }",
    "}",
];

fn file(cx: &App) -> AnyElement {
    let lines = (1..).zip(CODE).map(|(number, code)| parts::code_line(number, Stands::Same, code, OPENED, cx));
    parts::screen(cx)
        .child(
            bar("pull_requests.rs", "crates/hub/src/hub")
                .end(parts::tap("find", IconName::Search))
                .end(parts::tap("more", IconName::Ellipsis)),
        )
        .child(v_flex().flex_1().min_h_0().overflow_hidden().pt(Space::Small).children(lines))
        .child(parts::foot())
        .into_any_element()
}
