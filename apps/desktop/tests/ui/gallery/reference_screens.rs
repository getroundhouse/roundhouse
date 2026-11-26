//! A reply that links to issues and pull requests, each drawn as a chip from what the hub found of it, and the card a
//! chip shows under the pointer and the menu a right-click on it opens.

#![cfg(test)]

use client_core::fake_hub::{NOW_MS, World};
use look::Region;
use protocol::model::{
    About, ChecksRollup, GithubRepositoryName, Issue, IssueNumber, IssueState, IssueSummary, PrState, PullRequest,
    PullRequestLink, PullRequestLinked, Read, Reference, ReferenceLabel, RepoOrigin, RepoUrl, Review, SessionStatus,
    Summary,
};

use super::frame::{Frame, PROPOSAL, Part};
use super::proposal_chats;
use super::pull_requests;
use super::screens::FOCUS;
use super::steps::{Screen, Step, Target};
use super::worlds::chat_world;
use spec_screens::chats;

const ASK: &str = "Where are we with the sidebar's pull request badges?";

/// Links to each of [`references`], one of them twice, one with words of its own and one in bold, which its chip
/// doesn't take, and to `#1999`, which GitHub has no issue for, so it stays the link it's written as.
const REPLY: &str = "[#1158](https://github.com/example/roundhouse/pull/1158) is merged, so the hub reads every \
     workspace's pull request now, and [#1162](https://github.com/example/roundhouse/pull/1162) takes the sidebar's \
     last read of GitHub out of the client. Its checks are still running.\n\n\
     That leaves two edits waiting on your go-ahead:\n\n\
     - writing your decisions into [the badges' issue](https://github.com/example/roundhouse/issues/1021);\n\
     - **updating [#743](https://github.com/example/roundhouse/issues/743)**, which still records the old rule as \
     confirmed.\n\n\
     Once [#1021](https://github.com/example/roundhouse/issues/1021) has them, \
     [#1162](https://github.com/example/roundhouse/pull/1162) can say it closes it. The badge's colours follow \
     [example/signal-box#88](https://github.com/example/signal-box/issues/88), which is still open there, and \
     [#1999](https://github.com/example/roundhouse/issues/1999) must have been a typo.";

const DAY_MS: i64 = 24 * 60 * 60 * 1_000;

fn repository(name: &str) -> GithubRepositoryName {
    GithubRepositoryName::new("example", name).unwrap()
}

/// What an issue or a pull request in `example/{name}`, at `kind_in_url`, says of itself: opened by ana-p `days` ago,
/// with nothing more to it.
fn about(name: &str, kind_in_url: &str, number: u64, title: &str, days: i64) -> About {
    About {
        url: format!("https://github.com/example/{name}/{kind_in_url}/{number}"),
        title: title.to_string(),
        author: Some("ana-p".to_string()),
        opened_at_ms: NOW_MS - days * DAY_MS,
        comments: 0,
        labels: Vec::new(),
        body: String::new(),
    }
}

/// An issue in `example/{name}`.
fn issue(name: &str, number: u64, title: &str, state: IssueState, days: i64) -> Reference {
    Reference::Issue(Issue {
        repository: repository(name),
        number: IssueNumber::try_from(number).unwrap(),
        summary: Read {
            said: IssueSummary { about: about(name, "issues", number, title, days), state },
            read_at_ms: NOW_MS,
        },
    })
}

/// A pull request in this repository, standing as `change` leaves its summary, found by a chip, so with its summary
/// alone.
fn pull(number: u64, title: &str, days: i64, change: impl FnOnce(&mut Summary)) -> Reference {
    let fixture = protocol::fixtures::pull_request(number);
    let mut said = fixture.summary.said;
    said.about = about("roundhouse", "pull", number, title, days);
    change(&mut said);
    Reference::PullRequest(PullRequest {
        repository: repository("roundhouse"),
        summary: Read { said, read_at_ms: NOW_MS },
        history: None,
        stack: None,
        checks: None,
        ..fixture
    })
}

/// `reference`, saying more of itself as `change` has it.
fn saying(mut reference: Reference, change: impl FnOnce(&mut About)) -> Reference {
    match &mut reference {
        Reference::Issue(issue) => change(&mut issue.summary.said.about),
        Reference::PullRequest(pull_request) => change(&mut pull_request.summary.said.about),
        Reference::Unknown => {}
    }
    reference
}

/// What the hub found of each issue and pull request the reply links to.
fn references() -> Vec<Reference> {
    let label = |name: &str, color: &str| ReferenceLabel { name: name.to_string(), color: color.to_string() };
    let changes = |state, (added, removed), rollup, review| {
        move |said: &mut Summary| {
            (said.state, said.added, said.removed, said.rollup, said.review) =
                (state, added, removed, Some(rollup), review);
        }
    };
    let merged = changes(PrState::Merged, (412, 96), ChecksRollup::Passed, Review::Approved);
    let open = changes(PrState::Open, (188, 241), ChecksRollup::Running, Review::Required);
    vec![
        pull(1158, "Read every workspace's pull request in the hub", 6, merged),
        saying(pull(1162, "Take the sidebar's last read of GitHub out of the client", 1, open), |about| {
            about.comments = 2;
            about.body = "The sidebar asked GitHub for each workspace's pull request on its own. It now shows what \
                          the hub keeps, so every client agrees and GitHub is asked once."
                .to_string();
        }),
        saying(
            issue("roundhouse", 1021, "Decide how the sidebar's badges show a pull request", IssueState::Open, 12),
            |about| {
                about.comments = 6;
                about.labels =
                    vec![label("design", "d876e3"), label("sidebar", "0e8a16"), label("needs decision", "fbca04")];
                about.body = "A badge says where a workspace's pull request stands. Which states it shows, in what \
                          colours, and what a click on it does are still open."
                    .to_string();
            },
        ),
        issue("roundhouse", 743, "Record which pull request states the sidebar shows", IssueState::Done, 40),
        issue("signal-box", 88, "Name the badge colours", IssueState::Open, 3),
    ]
}

/// Puts each of `world`'s repositories on GitHub, as `example/{name}`, so a chip names one in the chat's own by its
/// number alone.
fn on_github(world: &mut World) {
    for repo in &mut world.repos {
        let path = repo.origin.checkout().unwrap_or_default().to_string();
        let url = RepoUrl::try_from(format!("https://github.com/example/{}", repo.name)).unwrap();
        repo.origin = RepoOrigin::Linked { path, url };
    }
}

/// The screen's chat, whose reply links to [`references`], with #1162 open in birch.
fn named() -> World {
    let mut world = chat_world(SessionStatus::Idle, chats::answered(ASK, REPLY));
    on_github(&mut world);
    world.github_says.references = references();
    let birch = world.workspaces.iter().find(|workspace| workspace.name == "birch").unwrap().id.clone();
    let url = "https://github.com/example/roundhouse/pull/1162".to_string();
    let mut pull_request = pull_requests::open();
    pull_request.pull_request.summary.said.about.url.clone_from(&url);
    // Which pull request it's on is what ties a chip to it, as the hub keeps it.
    let (repository, number) = (repository("roundhouse"), IssueNumber::try_from(1162).unwrap());
    let link = PullRequestLink { repository, number, url };
    world.github_says.pull_requests.insert(birch.clone(), pull_request);
    world.workspace_mut(&birch).pull_request_link = Some(PullRequestLinked { link: Some(link) });
    world
}

/// The proposal of the gallery's proposal screens, whose context links to #1021 and #1162, with [`references`] on
/// GitHub.
fn proposal_naming() -> World {
    let mut world = proposal_chats::world();
    on_github(&mut world);
    for proposal in world.proposals.values_mut().flatten() {
        proposal.current.proposed.context.push_str(
            " It's what [#1021](https://github.com/example/roundhouse/issues/1021) asks for, and \
             [#1162](https://github.com/example/roundhouse/pull/1162) has to merge first.",
        );
    }
    world.github_says.references = references();
    world
}

const CHAT: Frame = Frame::Transcript(&[]);
const CARD: Frame = Frame::Transcript(&[Part::Region(Region::ReferenceCard)]);
const MENU: Frame = Frame::Transcript(&[Part::Menu]);
const CARD_AND_MENU: Frame = Frame::Transcript(&[Part::Region(Region::ReferenceCard), Part::Menu]);

pub(crate) const SCREENS: &[Screen] = &[
    Screen { name: "references", spec: "chats.md", world: Some(named), frame: CHAT, steps: &[Step::Select(FOCUS)] },
    Screen {
        name: "references-in-a-proposal",
        spec: "chats.md",
        world: Some(proposal_naming),
        frame: PROPOSAL,
        steps: &[Step::Select(FOCUS), Step::Click("open-proposal/8d0c6a5e-proposal"), Step::MoveAway],
    },
    Screen {
        name: "reference-card-issue",
        spec: "chats.md",
        world: Some(named),
        frame: CARD,
        steps: &[
            Step::Select(FOCUS),
            Step::Hover { on: Target::Id("reference/#1021"), shows: Region::ReferenceCard.id() },
        ],
    },
    Screen {
        name: "reference-card-pull-request",
        spec: "chats.md",
        world: Some(named),
        frame: CARD,
        steps: &[
            Step::Select(FOCUS),
            Step::Hover { on: Target::Id("reference/#1162"), shows: Region::ReferenceCard.id() },
        ],
    },
    Screen {
        name: "reference-menu",
        spec: "chats.md",
        world: Some(named),
        frame: MENU,
        steps: &[Step::Select(FOCUS), Step::RightClick("reference/#1162")],
    },
    Screen {
        name: "reference-card-menu",
        spec: "chats.md",
        world: Some(named),
        frame: CARD_AND_MENU,
        steps: &[
            Step::Select(FOCUS),
            Step::Hover { on: Target::Id("reference/#1162"), shows: Region::ReferenceCard.id() },
            Step::Click("reference-more"),
        ],
    },
];
