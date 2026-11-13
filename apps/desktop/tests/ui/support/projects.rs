//! Projects as the tests find them: a window with two, each with workspaces, and a project's row in the sidebar.

use gpui_kit::{ElementId, SharedString, TestAppContext};
use protocol::model::{RepoId, WorkspaceId};

use super::{Stack, Ui, git, wait_out_setup};

/// A project's row in the sidebar.
pub(crate) fn project_row(repo: &RepoId) -> ElementId {
    ElementId::Name(SharedString::from(format!("project-{repo}")))
}

/// A window with two projects, each with two workspaces. The projects are returned in the order the sidebar lists
/// them, and their workspaces from the top of the sidebar down: nobody has worked in any, so each project's newest is
/// first.
pub(crate) fn two_projects<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> (Ui<'a>, [RepoId; 2], [WorkspaceId; 4]) {
    let client = stack.client();
    let other = stack.dir.path().join("other");
    std::fs::create_dir_all(&other).unwrap();
    git(&other, &["init", "-q", "-b", "main"]);
    std::fs::write(other.join("a.txt"), "one\n").unwrap();
    git(&other, &["add", "."]);
    git(&other, &["commit", "-qm", "first"]);
    for path in [stack.repo(), other] {
        client.repo_add(path.to_str().unwrap()).unwrap();
    }
    let mut ui = Ui::open(cx, stack);
    ui.wait("both repositories", |ui| ui.view().records().repos().len() == 2);
    let repos: Vec<RepoId> = ui.view().records().repos().iter().map(|repo| repo.id.clone()).collect();
    let make = |repo: &RepoId| client.workspace_create(repo, None, None, None).unwrap().id;
    let [lower, upper] = [make(&repos[0]), make(&repos[0])];
    let [last, third] = [make(&repos[1]), make(&repos[1])];
    ui.wait("every workspace", |ui| ui.view().visible_workspaces().len() == 4);
    wait_out_setup(&mut ui);
    (ui, repos.try_into().unwrap(), [upper, lower, third, last])
}
