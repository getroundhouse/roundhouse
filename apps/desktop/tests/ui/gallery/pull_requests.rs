//! Every state a pull request's bar can show, in one picture: the bar as the top of the sidebar of changes shows
//! it, and beside it what the header shows while the sidebar is hidden. Each is drawn by the app's own bar, from a
//! made-up pull request that stands that way, so the picture can't show a state the app doesn't reach.

#![cfg(test)]

use std::sync::Arc;

use desktop::busy::Busy;
use desktop::pull_request::{self, Lookup};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    Context, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, PlatformTextSystem, Render,
    Styled as _, Window, div, px, size,
};
use protocol::model::{
    Local, MergeState, OwnPullRequest, PrState, QueueState, Review, Standing, Summary, WorkspaceStatus,
};

use super::capture::Picture;
use super::headless;

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "pull-request-states";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "workspaces.md";
/// As wide as the sidebar of changes usually is, `Side::Right.usual_width()`.
const SIDEBAR: f32 = 432.;
/// Enough for the widest number and action the header shows.
const HEADER: f32 = 300.;
const ROW: f32 = 40.;
const GAP: f32 = 8.;
const MARGIN: f32 = 16.;

/// An open pull request with nothing in the way of merging it, with 9 checks passed, which each state changes one
/// thing about.
pub(crate) fn open() -> OwnPullRequest {
    let mut pull_request = protocol::fixtures::pull_request(327);
    let about = &mut pull_request.summary.said.about;
    about.url = "https://example.com/pull/327".to_string();
    about.title = "Queue prompts with Cmd-Enter".to_string();
    checked(OwnPullRequest { pull_request, local: Local::default() }, 9, 0, 0)
}

/// `own` with this many checks passed, failed and still running.
pub(crate) fn checked(own: OwnPullRequest, passed: usize, failed: usize, pending: usize) -> OwnPullRequest {
    OwnPullRequest { pull_request: protocol::fixtures::checked(own.pull_request, passed, failed, pending), ..own }
}

/// [`open`], with its summary changed by `change`.
fn with(change: impl FnOnce(&mut Summary)) -> Lookup {
    let mut own = open();
    change(&mut own.pull_request.summary.said);
    Lookup::Found(Some(Box::new(own)))
}

/// [`open`], changed by `change`.
fn with_own(change: impl FnOnce(&mut OwnPullRequest)) -> Lookup {
    let mut own = open();
    change(&mut own);
    Lookup::Found(Some(Box::new(own)))
}

/// In the order the app looks for them, after the two with no pull request to show, and then the one it can't tell.
/// Merged shows twice more: while the app archives another workspace, which greys out its Archive, and while this one
/// is being archived, when it says Archiving…, from its record. Every other state is drawn with nothing under way.
fn states() -> Vec<State> {
    let merged = || with(|pr| pr.state = PrState::Merged);
    let free = [
        Lookup::Looking,
        Lookup::Found(None),
        merged(),
        with(|pr| pr.state = PrState::Closed),
        with(|pr| pr.queue = Some(QueueState::AwaitingChecks)),
        with(|pr| pr.queue = Some(QueueState::Queued)),
        with(|pr| pr.queue = Some(QueueState::Unmergeable)),
        with_own(|own| own.local.uncommitted = 3),
        with_own(|own| own.local = Local { uncommitted: 0, ahead: 1, behind: 2 }),
        with_own(|own| own.local.behind = 1),
        with_own(|own| own.local.ahead = 2),
        with(|pr| pr.merge_state = MergeState::Dirty),
        with(|pr| pr.review = Review::ChangesRequested),
        with(|pr| pr.is_draft = true),
        with_own(|own| own.pull_request.checks = None),
        with_own(|own| *own = checked(open(), 6, 2, 1)),
        with_own(|own| *own = checked(open(), 6, 0, 3)),
        with(|pr| pr.review = Review::Required),
        with(|pr| pr.merge_state = MergeState::Blocked),
        with(|pr| pr.merge_state = MergeState::Computing),
        with(|_| {}),
        with(|pr| pr.merge_state = MergeState::Unknown),
    ];
    let mut states: Vec<_> = free.into_iter().map(|lookup| (lookup, Busy::Free, WorkspaceStatus::Ready)).collect();
    states.insert(3, (merged(), Busy::Moving, WorkspaceStatus::Ready));
    states.insert(4, (merged(), Busy::Moving, WorkspaceStatus::Archiving));
    states
}

/// Names every kind of standing, as patterns, in one match, so a new kind of standing doesn't compile until it's
/// listed here, and then [`missing`] says so until the picture draws it.
macro_rules! kinds {
    ($($kind:pat),+ $(,)?) => {
        const KINDS: &[&str] = &[$(stringify!($kind)),+];

        fn kind(standing: Standing) -> &'static str {
            match standing {
                $($kind => stringify!($kind)),+
            }
        }
    };
}

kinds![
    Standing::NoPullRequest,
    Standing::Merged,
    Standing::Closed,
    Standing::Queued(_),
    Standing::Uncommitted(_),
    Standing::Diverged,
    Standing::Behind(_),
    Standing::Unpushed(_),
    Standing::Conflicts,
    Standing::ChangesRequested,
    Standing::Draft,
    Standing::ChecksFailed(_),
    Standing::ChecksRunning(_),
    Standing::Reading,
    Standing::ReviewRequired,
    Standing::Blocked,
    Standing::Checking,
    Standing::Ready,
    Standing::Unknown,
];

/// The kinds of standing the picture doesn't draw.
pub(crate) fn missing() -> Vec<&'static str> {
    let drawn: Vec<&str> = states().iter().filter_map(|(lookup, ..)| lookup.standing()).map(kind).collect();
    KINDS.iter().copied().filter(|k| !drawn.contains(k)).collect()
}

/// A pull request as the bar finds it, what the app is waiting on the hub for, and the status of the workspace the bar
/// is for.
type State = (Lookup, Busy, WorkspaceStatus);

struct Sheet {
    states: Vec<State>,
}

impl Render for Sheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = look::colors(cx);
        let (pane, sidebar) = (colors.pane, colors.sidebar);
        let rows = self.states.iter().enumerate().map(|(index, (lookup, busy, status))| {
            let doing = client_core::moving::doing(*status);
            let bar = pull_request::bar(lookup, busy, doing, |_, _, _| {}, cx);
            let controls = pull_request::controls(lookup, busy, doing, |_, _, _| {});
            h_flex()
                .id(ElementId::NamedInteger("state".into(), index as u64))
                .gap(px(MARGIN))
                .child(div().w(px(SIDEBAR)).bg(sidebar).child(bar))
                .child(h_flex().w(px(HEADER)).h(px(ROW)).px_3().justify_end().children(controls))
        });
        v_flex().size_full().bg(pane).p(px(MARGIN)).gap(px(GAP)).children(rows)
    }
}

/// At the app's scale, as the screens are.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> Picture {
    let mut cx = headless::app(text_system);
    #[expect(clippy::cast_precision_loss, reason = "a couple of dozen rows")]
    let rows = states().len() as f32;
    let width = px(MARGIN * 3. + SIDEBAR + HEADER);
    let height = px(MARGIN * 2. + rows * ROW + (rows - 1.) * GAP);
    headless::draw(&mut cx, size(width, height), |_| Sheet { states: states() })
}
