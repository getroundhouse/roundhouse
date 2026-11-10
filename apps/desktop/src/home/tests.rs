use protocol::model::{AgentKind, InstanceId, Rev, Session, SessionStatus, SetupStatus, TitleSource, Workspace};
use protocol::model::{RepoId, WorkspaceStatus};

use super::{ArchiveFilter, Sort, last_active, listed};

fn workspace(name: &str, repo: &str, status: WorkspaceStatus, created_at_ms: i64) -> Workspace {
    Workspace {
        id: name.into(),
        repo_id: repo.into(),
        name: name.to_string(),
        branch: format!("branch-{name}"),
        base_ref: None,
        path: None,
        status,
        provider: "local".to_string(),
        instance_id: InstanceId::from("local"),
        created_at_ms,
        archived_at_ms: None,
        setup: SetupStatus::NotStarted,
        port_base: 0,
        rev: Rev::default(),
        title: None,
        title_source: TitleSource::default(),
        pull_request: None,
    }
}

fn session(workspace: &str, updated_at_ms: i64) -> Session {
    Session {
        id: format!("{workspace}-chat").into(),
        workspace_id: workspace.into(),
        agent: AgentKind::Claude,
        model: None,
        permission_mode: None,
        effort: None,
        status: SessionStatus::Idle,
        last_seq: 0,
        created_at_ms: 0,
        updated_at_ms,
        title: None,
        closed_at_ms: None,
        reply_seq: 0,
        read_seq: 0,
        background_tasks: Vec::new(),
        context: None,
        queued: Vec::new(),
        queue_held: false,
        turn_started_at_ms: None,
        rev: Rev::default(),
        title_source: TitleSource::FirstLine,
    }
}

fn names(workspaces: &[Workspace], listed: &[usize]) -> Vec<String> {
    listed.iter().map(|&i| workspaces[i].name.clone()).collect()
}

#[test]
fn it_lists_the_workspace_last_worked_in_first_and_one_never_worked_in_by_when_it_was_made() {
    use WorkspaceStatus::Ready;
    let workspaces =
        [workspace("alder", "r", Ready, 10), workspace("birch", "r", Ready, 30), workspace("cedar", "r", Ready, 20)];
    let chats = [session("alder", 40)];
    let active = |w: &Workspace| last_active(w, if w.name == "alder" { &chats } else { &[] });

    let order = listed(&workspaces, active, None, ArchiveFilter::Hide, Sort::LastActive, "");

    assert_eq!(names(&workspaces, &order), ["alder", "birch", "cedar"]);
}

#[test]
fn it_sorts_by_when_each_was_made_or_by_name_in_any_case() {
    use WorkspaceStatus::Ready;
    let workspaces =
        [workspace("cedar", "r", Ready, 30), workspace("alder", "r", Ready, 10), workspace("Birch", "r", Ready, 20)];
    let made = |w: &Workspace| w.created_at_ms;

    let created = listed(&workspaces, made, None, ArchiveFilter::Hide, Sort::Created, "");
    let by_name = listed(&workspaces, made, None, ArchiveFilter::Hide, Sort::Name, "");

    assert_eq!(names(&workspaces, &created), ["cedar", "Birch", "alder"]);
    assert_eq!(names(&workspaces, &by_name), ["alder", "Birch", "cedar"]);
}

#[test]
fn it_shows_archived_workspaces_only_when_asked_and_never_one_being_made() {
    use WorkspaceStatus::{Archived as Gone, Creating, Ready, Unknown};
    let workspaces = [
        workspace("ready", "r", Ready, 4),
        workspace("making", "r", Creating, 3),
        workspace("gone", "r", Gone, 2),
        workspace("strange", "r", Unknown, 1),
    ];
    let made = |w: &Workspace| w.created_at_ms;
    let shown = |archived| names(&workspaces, &listed(&workspaces, made, None, archived, Sort::Created, ""));

    assert_eq!(shown(ArchiveFilter::Hide), ["ready"]);
    assert_eq!(shown(ArchiveFilter::Show), ["ready", "gone"]);
    assert_eq!(shown(ArchiveFilter::Only), ["gone"]);
}

#[test]
fn it_narrows_to_a_repository_and_to_what_the_search_matches() {
    use WorkspaceStatus::Ready;
    let mut titled = workspace("elm", "other", Ready, 1);
    titled.title = Some(protocol::model::Title::typed("Fix the Login bug").unwrap());
    let workspaces = [workspace("alder", "r", Ready, 3), workspace("logins", "r", Ready, 2), titled];
    let made = |w: &Workspace| w.created_at_ms;
    let repo = RepoId::from("r");

    let in_repo = listed(&workspaces, made, Some(&repo), ArchiveFilter::Hide, Sort::Created, "");
    let searched = listed(&workspaces, made, None, ArchiveFilter::Hide, Sort::Created, " LOGIN ");
    let by_branch = listed(&workspaces, made, None, ArchiveFilter::Hide, Sort::Created, "branch-ald");

    assert_eq!(names(&workspaces, &in_repo), ["alder", "logins"]);
    assert_eq!(names(&workspaces, &searched), ["logins", "elm"]);
    assert_eq!(names(&workspaces, &by_branch), ["alder"]);
}
