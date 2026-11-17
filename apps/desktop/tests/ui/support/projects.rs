//! Projects as the tests find them: a window with two, each with workspaces, on a real hub or a fake one, and a
//! project's row in the sidebar.

use client_core::fake_hub::World;
use gpui_kit::{ElementId, SharedString, TestAppContext};
use protocol::model::{RepoId, SessionStatus, WorkspaceId};

use super::{FakeHub, Stack, Ui, git, wait_out_setup};

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

/// [`two_projects`] as a world for a fake hub, for a test of what the app keeps of its sidebar, such as which projects
/// are collapsed and their order, which asks nothing of a hub. Each workspace has a chat, made with it, which counts
/// as working in it, so each is the one last worked in when it's made, as a new one is. The projects and their
/// workspaces are returned as [`two_projects`] returns them, and the projects are listed by name.
pub(crate) fn two_projects_world() -> (World, [RepoId; 2], [WorkspaceId; 4]) {
    let mut world = World::default();
    let repos = [world.repo("ledger"), world.repo("signals")];
    let [lower, upper] = [with_a_chat(&mut world, &repos[0], "lower"), with_a_chat(&mut world, &repos[0], "upper")];
    let [last, third] = [with_a_chat(&mut world, &repos[1], "last"), with_a_chat(&mut world, &repos[1], "third")];
    (world, repos, [upper, lower, third, last])
}

/// A workspace in `repo`, made after every other in `world`, with a chat that's had nothing said in it yet.
pub(crate) fn with_a_chat(world: &mut World, repo: &RepoId, name: &str) -> WorkspaceId {
    let workspace = world.workspace(repo, name);
    world.chat(&workspace, SessionStatus::Idle, Vec::new());
    workspace
}

/// The window, open on a fake hub holding `world`. The hub goes with the test, so it's returned to be kept.
pub(crate) fn open_on_the_world(cx: &mut TestAppContext, world: World) -> (FakeHub, Ui<'_>) {
    let fake = FakeHub::holding(world);
    let ui = Ui::open_on(cx, &fake);
    (fake, ui)
}
