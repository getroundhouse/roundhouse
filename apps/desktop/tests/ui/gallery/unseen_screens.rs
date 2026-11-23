//! The screens of a workspace whose repository GitHub shows the default none of, as for a private one in an
//! organization that hasn't approved Roundhouse: the warning in the card the workspace's name opens, and the pull
//! request's bar, which says it can't see the pull request, with the warning open from it.

#![cfg(test)]

use look::Region;
use protocol::model::{GithubLoginKind, GithubRepositoryName, RepositoryUnseen};

use super::frame::{Frame, Part, WORKSPACE_CARD};
use super::screens::FOCUS;
use super::steps::{Screen, Step, Target};
use super::worlds::diff;
use client_core::fake_hub::World;

pub(crate) const SCREENS: &[Screen] = &[
    Screen {
        name: "workspace-card-warning",
        spec: "workspaces.md",
        world: Some(unseen),
        frame: WORKSPACE_CARD,
        steps: &[
            Step::Select(FOCUS),
            Step::Hover { on: Target::Id("workspace-title"), shows: Region::WorkspaceCard.id() },
        ],
    },
    Screen {
        name: "pull-request-unseen",
        spec: "workspaces.md",
        world: Some(unseen),
        frame: Frame::Exactly(Part::Region(Region::RightSidebar), &[Part::Region(Region::WarningsCard)]),
        steps: &[
            Step::Select(FOCUS),
            Step::Hover { on: Target::Id("pull-request-unseen"), shows: Region::WarningsCard.id() },
        ],
    },
];

/// GitHub unseen as `ada`, signed in with GitHub, so the warning offers to approve Roundhouse, as the hub says
/// for a repository GitHub shows a sign-in's token none of.
pub(crate) fn signed_in_unseen() -> RepositoryUnseen {
    RepositoryUnseen {
        repository: GithubRepositoryName::named("railworks/roundhouse"),
        username: "ada".into(),
        login: GithubLoginKind::SignIn,
        installation: None,
        said: "Could not resolve to a Repository with the name 'railworks/roundhouse'.".into(),
        approve_at: Some("https://github.com/settings/connections/applications/a-made-up-client-id".into()),
    }
}

/// A workspace with changes and no pull request known, whose repository GitHub showed `ada` none of.
fn unseen() -> World {
    let mut world = diff();
    let focus = world.workspaces.iter().find(|workspace| workspace.name == FOCUS).unwrap().id.clone();
    world.github_says.unseen.insert(focus, signed_in_unseen());
    world
}
