//! The order of the sidebar's projects, which dragging a project's row changes. The project's row and its workspaces
//! follow the pointer up and down the list as one, on a raised surface over the others, and the project takes its new
//! place among them as it covers half of each, so they slide out of its way. Held near the top or bottom of the list,
//! it scrolls the list on, so a project can be taken past what's in sight. Let go, it slides the rest of the way to
//! where it has got to, and the order is saved as a setting, so it lasts.

use std::iter;
use std::ops::Range;

use gpui_kit::{
    AnyElement, App, Bounds, Context, CursorStyle, DragMoveEvent, Empty, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, Styled as _, TestSupportExt as _, Window, div, point, px,
};
use look::motion::ScrollingOn;
use protocol::model::{Repo, RepoId};

use super::fold::Drawn;
use super::{GROUP_GAP, Kept, LIST_PAD, Line, LineId};
use crate::app::AppView;
use crate::reorder;
use crate::settings::Settings;

/// How far the surface under a dragged project reaches past its lines, which is into the space around the list at
/// its sides.
const AROUND: Pixels = px(4.);
/// How near the top or bottom of the list a dragged project has to be held for the list to scroll on. Inside the
/// list, since its project is drawn there, and a project held past the edge is out of sight.
const NEAR_THE_EDGE: Pixels = px(24.);

/// The projects' places in `repos`, in the order the sidebar lists them: those `order` names, in its order, and then
/// any it doesn't, such as one added since, in the hub's.
pub(super) fn ordered(repos: &[Repo], order: &[RepoId]) -> Vec<usize> {
    let place = |repo: &Repo| order.iter().position(|id| *id == repo.id).unwrap_or(usize::MAX);
    let mut ordered: Vec<usize> = (0..repos.len()).collect();
    ordered.sort_by_key(|&at| place(&repos[at]));
    ordered
}

/// The order to save once a drag has left the projects listed in `dragged`: that, and then the projects `saved`
/// names that aren't listed, in the order they were. The app can show several hubs, so one of those may be another
/// hub's, whose order this mustn't lose.
pub(super) fn merged(dragged: Vec<RepoId>, saved: &[RepoId]) -> Vec<RepoId> {
    let others: Vec<RepoId> = saved.iter().filter(|id| !dragged.contains(id)).cloned().collect();
    [dragged, others].concat()
}

/// How far down the list each project's lines reach, at rest, from its row's top to its last line's bottom, in the
/// order they're listed. That leaves out the gap below it, which every project has but the last, so a project's reach
/// doesn't depend on where it is. `places` has where each of `resting`'s lines goes, and `id` names the project a
/// `Line::Repo` is.
pub(super) fn groups(
    resting: &[Line],
    places: &[Pixels],
    id: impl Fn(usize) -> RepoId,
) -> Vec<(RepoId, Range<Pixels>)> {
    let rows = resting.iter().enumerate().filter_map(|(at, line)| match *line {
        Line::Repo(repo) => Some((id(repo), at)),
        Line::Heading | Line::Workspace(_) | Line::Creating(_) => None,
    });
    let reach = |(id, row): (RepoId, usize)| {
        let last = with_its_workspaces(resting, row, |&line| line).end - 1;
        (id, places[row]..places[last] + resting[last].height())
    };
    rows.map(reach).collect()
}

/// How far down the list each of a run of projects reaches, as tall as `heights` say, one under another from `top`,
/// with the space kept between projects.
pub(super) fn stack(heights: impl IntoIterator<Item = Pixels>, top: Pixels) -> Vec<Range<Pixels>> {
    let mut top = top;
    let below = |height: Pixels| {
        let reach = top..top + height;
        top = reach.end + GROUP_GAP;
        reach
    };
    heights.into_iter().map(below).collect()
}

/// Where among `groups`, each project's reach down the list in the order listed, the one at `dragged` goes with its
/// row's top `top` down the list: after every other project whose middle that's past, in the list as it would be
/// without the dragged one. So it changes places with the project beside it once it covers half of it, whichever way
/// it's going, and since where it goes doesn't depend on where it is, it can't go back and forth under a pointer
/// that's holding still.
pub(super) fn slot(groups: &[Range<Pixels>], dragged: usize, top: Pixels) -> usize {
    let others = groups.iter().enumerate().filter(|&(at, _)| at != dragged).map(|(_, reach)| reach.end - reach.start);
    let middle = |reach: &Range<Pixels>| reach.start + (reach.end - reach.start) / 2.;
    stack(others, groups[0].start).iter().filter(|reach| middle(reach) <= top).count()
}

/// What a drag of a project's row carries, which says which project it is.
pub(super) struct Dragged(pub(super) RepoId);

/// A drag of a project shows nothing under the pointer, since the project's own lines follow it.
impl Render for Dragged {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

/// A project that's being dragged, or has been let go and is on its way to its place, over the others until it's
/// there.
pub(crate) struct Dragging {
    pub(super) project: RepoId,
    /// How far below its row's top the pointer took hold of it.
    grip: Pixels,
    /// The projects' order, from the top, once it has changed places with another. It's saved once it's let go, so the
    /// settings change once for a drag, rather than at every project it passes.
    order: Option<Vec<RepoId>>,
    /// How far down the list its row's top is, by where the pointer has got to. `None` until the pointer moves.
    top: Option<Pixels>,
    /// Whether it's still held. Once it isn't, it's drawn where it has got to on its way to its place.
    held: bool,
    /// How far down the window the pointer is, and the list's box, as of the pointer's last move while the list was
    /// drawn.
    pointer: Option<(Pixels, Bounds<Pixels>)>,
    /// The list scrolling on under it, while it's held near the list's top or bottom.
    scrolling: ScrollingOn,
}

impl Dragging {
    /// A project the pointer has just taken hold of, `grip` below its row's top.
    pub(super) fn held(project: RepoId, grip: Pixels) -> Self {
        let scrolling = ScrollingOn::default();
        Self { project, grip, order: None, top: None, held: true, pointer: None, scrolling }
    }

    /// The list isn't drawn, so the pointer can't be over it, and it doesn't scroll on until the pointer moves again once
    /// it's drawn. The drag goes on, and ends when it's let go.
    pub(super) fn not_drawn(&mut self) {
        self.pointer = None;
        self.scrolling = ScrollingOn::default();
    }
}

/// The lines of the project whose row is at `row`: its row and then its workspaces'.
fn with_its_workspaces<T>(lines: &[T], row: usize, line: impl Fn(&T) -> Line) -> Range<usize> {
    row..row + 1 + lines[row + 1..].iter().take_while(|&next| line(next).is_workspace()).count()
}

/// Where the surface under a dragged project's lines is in the list, from how far down they reach. It's a place of its
/// own, over the other projects and under the dragged one's lines, so it shows wherever they do, even while the row
/// is out of sight.
pub(super) fn surface_drawn(reach: &Range<Pixels>) -> reorder::Drawn {
    reorder::Drawn { top: reach.start - AROUND, height: reach.end - reach.start + AROUND * 2., above: true }
}

/// The surface under a dragged project's lines, into the space around the list at its sides.
pub(super) fn surface(cx: &App) -> AnyElement {
    let raised = look::raised(cx).absolute().top_0().bottom_0().left(-AROUND).right(-AROUND);
    div().id("dragged-project").size_full().child(raised).test_support().into_any_element()
}

impl Kept {
    /// The projects' order, from the top: as it's being dragged, or as saved.
    pub(super) fn project_order<'a>(&'a self, settings: &'a Settings) -> &'a [RepoId] {
        let dragged = self.dragging.as_ref().and_then(|dragging| dragging.order.as_deref());
        dragged.unwrap_or(&settings.project_order)
    }

    /// Once the lines are laid out: draws the dragged project's, whose row is at `row`, where the pointer has them,
    /// and forgets a project that was let go once it's back in its place. Says how far down the list its lines reach
    /// as they're drawn, which the surface under them covers.
    pub(super) fn carry(&mut self, lines: &mut [Drawn], row: Option<usize>) -> Option<Range<Pixels>> {
        let dragging = self.dragging.as_ref()?;
        let Some(row) = row else {
            self.dragging = None;
            return None;
        };
        let its = with_its_workspaces(lines, row, |drawn| drawn.line);
        let its = &mut lines[its];
        match dragging.top.filter(|_| dragging.held) {
            Some(top) => {
                let by = top - its[0].top;
                for drawn in its.iter_mut() {
                    drawn.top += by;
                }
            }
            None if !dragging.held && !its[0].moving => {
                self.dragging = None;
                return None;
            }
            None => {}
        }
        let last = its[its.len() - 1];
        Some(its[0].top..last.top + last.cut.unwrap_or_else(|| last.line.height()))
    }
}

impl AppView {
    /// The pointer moved while a project was being dragged, which it follows from the next frame.
    pub(super) fn drag_project(&mut self, event: &DragMoveEvent<Dragged>, window: &mut Window, cx: &mut Context<Self>) {
        cx.set_active_drag_cursor_style(CursorStyle::ClosedHand, window);
        let Some(dragging) = self.sidebar.dragging.as_mut().filter(|dragging| dragging.held) else { return };
        dragging.pointer = Some((event.event.position.y, event.bounds));
        cx.notify();
    }

    /// As each frame starts, before the sidebar's lines are worked out, since this can change the projects' order: once
    /// the drag of a project has ended, lets go of it, and while it's held, scrolls the list on under it and has it
    /// follow the pointer. It's done on each frame, rather than as the pointer moves, since the list scrolls on under a
    /// pointer that holds still, when no event comes.
    pub(super) fn keep_up_with_a_drag(&mut self, window: &mut Window, cx: &mut App) {
        self.let_go_of_a_project(cx);
        self.scroll_under_a_drag(window, cx);
        self.follow_the_pointer();
    }

    /// Once the kit has ended the drag of a project: saves the order it left, and has its lines slide to their places
    /// from where the pointer left them.
    fn let_go_of_a_project(&mut self, cx: &mut App) {
        let Some(dragging) = self.sidebar.dragging.as_mut().filter(|dragging| dragging.held && !cx.has_active_drag())
        else {
            return;
        };
        dragging.held = false;
        if let Some(order) = dragging.order.clone() {
            crate::settings::change(cx, |settings| settings.project_order = merged(order, &settings.project_order));
        }
        let (project, top) = (dragging.project.clone(), dragging.top);
        let rest = self.sidebar.groups.iter().find(|(id, _)| *id == project).map(|(_, reach)| reach.start);
        let (Some(top), Some(rest)) = (top, rest) else { return };
        let workspaces = self.records().workspaces().iter().filter(|w| w.repo_id == project);
        let workspaces = workspaces.map(|w| LineId::Workspace(w.id.clone()));
        let its: Vec<LineId> = iter::once(LineId::Repo(project.clone())).chain(workspaces).collect();
        for id in &its {
            self.sidebar.places.slide_back(id, top - rest, cx);
        }
    }

    /// While a project is held near the top or bottom of the list, scrolls the list on, faster the nearer the edge
    /// it's held, as far as the list's end.
    fn scroll_under_a_drag(&mut self, window: &mut Window, cx: &App) {
        let (now, offset) = (cx.background_executor().now(), self.sidebar_scroll.offset());
        let furthest = -self.sidebar_scroll.max_offset().y;
        let Some(dragging) = self.sidebar.dragging.as_mut().filter(|dragging| dragging.held) else { return };
        let Some((y, list)) = dragging.pointer else { return };
        let near = list.top() + NEAR_THE_EDGE..list.bottom() - NEAR_THE_EDGE;
        let further = if y < near.start { offset.y < px(0.) } else { offset.y > furthest };
        if !further {
            dragging.scrolling = ScrollingOn::default();
        } else if let Some(by) = dragging.scrolling.by(y, near, now) {
            self.sidebar_scroll.set_offset(point(offset.x, (offset.y - by).clamp(furthest, px(0.))));
            window.request_animation_frame();
        }
    }

    /// Puts the project being dragged where the pointer has it, whatever scrolled the list, the wheel too, as far as
    /// either end of the projects, and has it take the place [`slot`] says.
    fn follow_the_pointer(&mut self) {
        let scrolled = self.sidebar_scroll.offset().y;
        let Kept { groups, dragging, .. } = &mut self.sidebar;
        let Some(dragging) = dragging.as_mut().filter(|dragging| dragging.held) else { return };
        let (Some((y, area)), Some(from)) =
            (dragging.pointer, groups.iter().position(|(id, _)| *id == dragging.project))
        else {
            return;
        };
        let reaches: Vec<Range<Pixels>> = groups.iter().map(|(_, reach)| reach.clone()).collect();
        let first = reaches[0].start;
        let last = reaches[reaches.len() - 1].end - (reaches[from].end - reaches[from].start);
        let top = (y - area.top() - LIST_PAD - scrolled - dragging.grip).clamp(first, last.max(first));
        dragging.top = Some(top);
        let to = slot(&reaches, from, top);
        if to != from {
            let mut order: Vec<RepoId> = groups.iter().map(|(id, _)| id.clone()).collect();
            let moved = order.remove(from);
            order.insert(to, moved);
            dragging.order = Some(order);
        }
    }
}

#[cfg(test)]
mod tests;
