//! What the window's two sidebars share: each shows or hides with a slide, and is as wide as its inner edge was last
//! dragged to. The one on the left lists the workspaces, and the one on the right the changes. What's chosen for each
//! holds in every workspace until the app quits.
//!
//! What's beside a sidebar moves with it as it slides, so it's laid out again on every frame of the slide, and only
//! then. How far each has slid is kept here rather than by a view, because two views lay out by the one on the left:
//! the window, which draws it, and the workspace, whose header starts clear of the window's buttons. The window
//! works out where both are once, as it starts to draw a frame, and every view reads that, so no two of them
//! disagree by the moment between their asking.

use gpui_kit::base::HandleEdge;
use gpui_kit::component::resizable::resize_handle;
use gpui_kit::{
    AnyElement, App, AppContext as _, Axis, Context, DragMoveEvent, Empty, Global, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, Render, Styled as _, Window, div, px,
};

use look::motion::Sliding;
use look::{InRegion as _, Region};

use crate::haptics;

mod window_buttons;

use window_buttons::Clearance;
pub(crate) use window_buttons::{
    AtTheTop, WINDOW_BUTTONS_INSET, WINDOWS_BUTTON_WIDTH, corner, corner_inset, end_inset, end_room, header_inset,
};

/// How close to its usual width a dragged sidebar has to come to take it, so the usual width is easy to get back to.
/// A Mac's trackpad taps as it takes it, so it can be felt as well as seen.
const SNAP: Pixels = px(8.);
/// The least room the sidebars leave between them while the window has it, so a chat can still be read beside them.
const BETWEEN: Pixels = px(360.);

/// One of the two sidebars, by the edge of the window it's on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Side {
    /// The workspaces.
    Left,
    /// The changes.
    Right,
}

/// Whether a sidebar shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Sidebar {
    Open,
    Hidden,
}

/// What's chosen for one sidebar.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Chosen {
    pub(crate) sidebar: Sidebar,
    /// How wide its edge was dragged to, which it is while it's open and the window has the room. Its inner edge is
    /// included.
    width: Pixels,
    /// How far open it is, from 0 to 1, which is on its way for as long as it slides.
    open: Sliding,
}

/// What the window draws beside the sidebars in a frame, which they're fitted around.
#[derive(Clone, Copy, Default)]
pub(crate) struct Beside {
    /// Whether the sidebar on the right is there to draw: a workspace is showing, with changes to list. Otherwise it
    /// takes none of the window, whatever's chosen for it.
    pub(crate) changes: bool,
    /// Whether something sits above the header and fills the strip along the window's top, as an error does, so the
    /// header is below the window's buttons rather than beside them.
    pub(crate) above_the_header: bool,
}

/// Where both sidebars are in the frame being drawn.
#[derive(Clone, Copy, Default)]
struct Placed {
    beside: Beside,
    /// How wide each is, left then right, whether or not all of it shows.
    widths: (Pixels, Pixels),
    /// How much of each shows, from the window's edge.
    shown: (Pixels, Pixels),
    /// How far what's at the top of the window keeps from each end, to clear the window's buttons.
    clearance: Clearance,
}

/// What's chosen for both, which every workspace shares until the app quits.
#[derive(Clone, Copy)]
struct Sides {
    left: Chosen,
    right: Chosen,
    placed: Placed,
}

impl Global for Sides {}

impl Default for Sides {
    /// Both start open, so the workspaces, and a workspace's changes and where its pull request stands, are in view
    /// from the start.
    fn default() -> Self {
        let open = |side: Side| Chosen { sidebar: Sidebar::Open, width: side.usual_width(), open: Sliding::at(1.) };
        Self { left: open(Side::Left), right: open(Side::Right), placed: Placed::default() }
    }
}

impl Sides {
    fn of(cx: &App) -> Self {
        cx.try_global::<Sides>().copied().unwrap_or_default()
    }

    /// How wide each sidebar is in a window `across` wide, left then right, with each `open` this far, from 0 to 1.
    /// Each is as wide as chosen, unless both open would leave less than [`BETWEEN`] between them. Then the one on
    /// the right gives way first, since it has more to give, and then the one on the left, each no further than its
    /// narrowest. A window too small even for that squeezes what's between.
    ///
    /// A sidebar is as wide as it is with the other's room as much of it as shows, so one sliding away gives up its
    /// room as it goes and the other widens into it, and nothing jumps when a slide sets off or arrives. A sliding
    /// sidebar's own width goes by how far the other is open and not by how far it is, so it slides as one piece and
    /// nothing in it is laid out again on the way. A hidden one is as wide as it will be when it opens.
    fn fitted(self, across: Pixels, open: (f32, f32)) -> (Pixels, Pixels) {
        let room = across - BETWEEN;
        let (left_limits, right_limits) = (Side::Left.limits(), Side::Right.limits());
        // The one on the right as it is beside the one on the left all the way open.
        let right_beside_the_left = (room - self.left.width).clamp(right_limits.narrowest, self.right.width);
        let left = (room - right_beside_the_left * open.1).clamp(left_limits.narrowest, self.left.width);
        let right = (room - left * open.0).clamp(right_limits.narrowest, self.right.width);
        (left, right)
    }

    /// How much of the window the other sidebar keeps however far `side` is dragged, which goes by what's chosen
    /// and not by how the two are fitted now, so dragging an edge back gets back the width it had. The one on the
    /// right gives way to the one on the left, as far as its narrowest, and the one on the left gives way to nothing.
    fn kept_beside(self, side: Side) -> Pixels {
        let open = |chosen: Chosen| chosen.sidebar == Sidebar::Open;
        match side {
            Side::Left if open(self.right) && self.placed.beside.changes => Side::Right.limits().narrowest,
            Side::Right if open(self.left) => self.left.width,
            Side::Left | Side::Right => px(0.),
        }
    }
}

/// Works out where both sidebars are for the frame the window is starting to draw, with `beside` them, and asks for
/// the next frame for as long as either slides. Reduce Motion skips the slide.
pub(crate) fn place(beside: Beside, window: &mut Window, cx: &mut App) {
    let mut sides = Sides::of(cx);
    // The one on the right slides only where it's drawn.
    let right = if beside.changes { sides.right.open.draw(window, cx) } else { 0. };
    let open = (sides.left.open.draw(window, cx), right);
    let widths = sides.fitted(window.viewport_size().width, open);
    // How far open each is, rather than how wide, so a drag moves its edge at once. In whole pixels, so what's beside
    // it stays on them as it moves.
    let shown = ((widths.0 * open.0).round(), (widths.1 * open.1).round());
    let clearance = Clearance::with_shown(window, shown);
    sides.placed = Placed { beside, widths, shown, clearance };
    cx.set_global(sides);
}

/// How narrow and how wide a sidebar's edge drags.
struct Limits {
    /// Still fits a workspace's name, or a file's.
    narrowest: Pixels,
    widest: Pixels,
}

impl Side {
    /// Its inner edge included. As wide as the sidebars `docs/inspiration.md` measures, so the layout feels familiar,
    /// which on the right is wide enough for a long path and its counts on one line.
    const fn usual_width(self) -> Pixels {
        match self {
            Side::Left => px(290.),
            Side::Right => px(432.),
        }
    }

    const fn limits(self) -> Limits {
        match self {
            Side::Left => Limits { narrowest: px(220.), widest: px(480.) },
            Side::Right => Limits { narrowest: px(300.), widest: px(800.) },
        }
    }

    pub(crate) fn chosen(self, cx: &App) -> Chosen {
        let sides = Sides::of(cx);
        match self {
            Side::Left => sides.left,
            Side::Right => sides.right,
        }
    }

    fn choose(self, chosen: Chosen, cx: &mut App) {
        let mut sides = Sides::of(cx);
        match self {
            Side::Left => sides.left = chosen,
            Side::Right => sides.right = chosen,
        }
        cx.set_global(sides);
    }

    /// Shows it if it's hidden, and hides it if it shows, sliding from wherever it has got to. The view that draws it
    /// has to be told to draw again.
    pub(crate) fn toggle(self, cx: &mut App) {
        let mut chosen = self.chosen(cx);
        let (sidebar, open) = match chosen.sidebar {
            Sidebar::Open => (Sidebar::Hidden, 0.),
            Sidebar::Hidden => (Sidebar::Open, 1.),
        };
        chosen.sidebar = sidebar;
        chosen.open.go(open, cx);
        self.choose(chosen, cx);
    }

    /// How wide it is in the frame being drawn, which is as wide as chosen unless the window is too narrow for both.
    /// Settings stands in for the one on the left at this width.
    pub(crate) fn width(self, cx: &App) -> Pixels {
        let (left, right) = Sides::of(cx).placed.widths;
        match self {
            Side::Left => left,
            Side::Right => right,
        }
    }

    /// How much of it is drawn in this frame, from the window's edge: all of it while it's open, and less for as long
    /// as it slides. `None` once none of it shows.
    pub(crate) fn shown(self, cx: &App) -> Option<Pixels> {
        let (left, right) = Sides::of(cx).placed.shown;
        let shown = match self {
            Side::Left => left,
            Side::Right => right,
        };
        (shown > px(0.)).then_some(shown)
    }

    /// Takes the width its edge was dragged to, in a window `across` wide. Says how that changed it.
    fn drag_to(self, width: Pixels, across: Pixels, cx: &mut App) -> Change {
        let width = dragged(self, width, across - Sides::of(cx).kept_beside(self));
        let chosen = self.chosen(cx);
        let change = Change::of(self, chosen.width, width);
        if change != Change::Unchanged {
            self.choose(Chosen { width, ..chosen }, cx);
        }
        change
    }
}

/// How a drag of a sidebar's edge changed its width.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Change {
    Unchanged,
    /// It's as wide as its edge was dragged to.
    Dragged,
    /// It took its usual width, from another, which the trackpad taps for. Only arriving there taps, so the pointer
    /// moving on while it holds that width doesn't tap again.
    ToUsual,
}

impl Change {
    /// What going `from` one width `to` another is, for `side`.
    fn of(side: Side, from: Pixels, to: Pixels) -> Self {
        if to == from {
            Change::Unchanged
        } else if to == side.usual_width() {
            Change::ToUsual
        } else {
            Change::Dragged
        }
    }
}

/// The width a sidebar takes when its edge is dragged to `width`, with `room` for it and what's between the sidebars:
/// its usual width when that's close, and otherwise a whole number of pixels within its limits that leaves the
/// middle room. A window too small for that leaves it at its narrowest.
fn dragged(side: Side, width: Pixels, room: Pixels) -> Pixels {
    let Limits { narrowest, widest } = side.limits();
    let widest = widest.min(room - BETWEEN).max(narrowest);
    let usual = side.usual_width();
    if (width - usual).abs() <= SNAP && usual <= widest {
        return usual;
    }
    width.round().clamp(narrowest, widest)
}

/// What a drag of a sidebar's edge carries: whose edge it is.
#[derive(Clone, Copy)]
struct Resizing(Side);

/// A drag of an edge shows nothing under the pointer.
impl Render for Resizing {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

/// A sidebar in its place at the window's edge, `shown` wide, with the rest of it out of sight past the edge it slides
/// from, and its inner edge to drag. What's beside it has the rest of the room, so it moves as the sidebar slides.
pub(crate) fn frame(side: Side, shown: Pixels, content: impl IntoElement, cx: &App) -> AnyElement {
    let (region, handle, edge) = match side {
        Side::Left => (Region::LeftSidebar, "left-sidebar-edge", HandleEdge::Trailing),
        Side::Right => (Region::RightSidebar, "right-sidebar-edge", HandleEdge::Leading),
    };
    // The band along the sidebar's own edge, over its line, so the line it draws is the one already there.
    let handle = resize_handle(handle, Axis::Horizontal)
        .inside(edge)
        .on_drag(Resizing(side), |drag, _, _, cx| cx.new(|_| *drag));
    // Kept at its whole width while it slides, which `Sides::fitted` keeps the same, so nothing in it is laid out
    // again on the way.
    let whole = div().absolute().top_0().bottom_0().w(side.width(cx)).child(content).child(handle);
    let whole = match side {
        Side::Left => whole.right_0(),
        Side::Right => whole.left_0(),
    };
    let frame = div().region(region).relative().flex_none().h_full().w(shown).overflow_hidden().child(whole);
    // A drag is told to every element that listens for it, wherever the pointer is, so each sidebar takes only its own.
    frame
        .on_drag_move(move |event: &DragMoveEvent<Resizing>, window, cx| {
            if event.drag(cx).0 != side {
                return;
            }
            let width = match side {
                Side::Left => event.event.position.x - event.bounds.left(),
                Side::Right => event.bounds.right() - event.event.position.x,
            };
            match side.drag_to(width, window.viewport_size().width, cx) {
                Change::Unchanged => {}
                Change::Dragged => window.refresh(),
                Change::ToUsual => {
                    haptics::align();
                    window.refresh();
                }
            }
        })
        .into_any_element()
}

#[cfg(test)]
mod tests;
