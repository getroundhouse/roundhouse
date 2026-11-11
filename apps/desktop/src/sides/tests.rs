use gpui_kit::{Pixels, TestAppContext, px};
use look::motion::Sliding;

use super::{Beside, Change, Chosen, Placed, Side, Sidebar, Sides, dragged};

/// A window with room for either sidebar at its widest.
const WIDE: Pixels = px(2000.);

/// Both sidebars open at these widths, with a workspace showing between them.
fn sides(left: f32, right: f32) -> Sides {
    let chosen = |width: f32| Chosen { sidebar: Sidebar::Open, width: px(width), open: Sliding::at(1.) };
    let placed = Placed { beside: Beside { changes: true, above_the_header: false }, ..Placed::default() };
    Sides { left: chosen(left), right: chosen(right), placed }
}

#[test]
fn a_sidebar_dragged_close_to_its_usual_width_takes_it() {
    assert_eq!(dragged(Side::Left, px(283.), WIDE), px(290.));
    assert_eq!(dragged(Side::Left, px(298.), WIDE), px(290.));
    assert_eq!(dragged(Side::Right, px(425.5), WIDE), px(432.));

    assert_eq!(dragged(Side::Left, px(281.), WIDE), px(281.), "further off, it stays where it's dragged");
    assert_eq!(dragged(Side::Left, px(299.), WIDE), px(299.));
}

/// The trackpad taps as the edge arrives at its usual width, and not again while it holds it, nor as it leaves.
#[gpui_kit::test]
fn a_sidebars_edge_taps_once_as_it_takes_its_usual_width(cx: &mut TestAppContext) {
    use Change::{Dragged, ToUsual, Unchanged};
    let mut drag = |to: f32| cx.update(|cx| Side::Left.drag_to(px(to), WIDE, cx));

    // It starts at its usual width, so a drag that begins there doesn't tap. Then out, back in through the usual
    // width, past it, and back again.
    let changes: Vec<Change> = [292., 350., 298., 293., 286., 282., 270., 286.].map(&mut drag).into();

    assert_eq!(changes, [Unchanged, Dragged, ToUsual, Unchanged, Unchanged, Unchanged, Dragged, ToUsual]);
}

#[test]
fn a_sidebar_drags_to_whole_pixels_within_its_limits() {
    assert_eq!(dragged(Side::Left, px(350.4), WIDE), px(350.));
    assert_eq!(dragged(Side::Left, px(20.), WIDE), px(220.));
    assert_eq!(dragged(Side::Left, px(900.), WIDE), px(480.));
    assert_eq!(dragged(Side::Right, px(20.), WIDE), px(300.));
    assert_eq!(dragged(Side::Right, px(1900.), WIDE), px(800.));
}

#[test]
fn dragging_a_sidebar_leaves_room_between_the_two() {
    assert_eq!(dragged(Side::Right, px(700.), px(1000.)), px(640.));
    assert_eq!(dragged(Side::Left, px(470.), px(760.)), px(400.));

    assert_eq!(dragged(Side::Left, px(400.), px(500.)), px(220.), "a small window leaves it at its narrowest");
    assert_eq!(dragged(Side::Left, px(290.), px(600.)), px(240.), "its usual width doesn't fit, so it isn't taken");
}

/// Both all the way open.
const OPEN: (f32, f32) = (1., 1.);

#[test]
fn sidebars_are_as_wide_as_chosen_while_the_window_has_room_between_them() {
    assert_eq!(sides(290., 432.).fitted(px(1320.), OPEN), (px(290.), px(432.)));
    assert_eq!(sides(290., 432.).fitted(px(1082.), OPEN), (px(290.), px(432.)), "exactly enough");
    // Each widened while the other was hidden, to more than the window has for both. The one showing is as wide as
    // it was dragged, and the one hidden as wide as it will be when it opens.
    assert_eq!(sides(480., 800.).fitted(px(1320.), (1., 0.)), (px(480.), px(480.)));
    assert_eq!(sides(480., 800.).fitted(px(1320.), (0., 1.)), (px(480.), px(800.)));
}

#[test]
fn in_a_window_too_narrow_for_both_the_right_sidebar_gives_way_first_and_then_the_left() {
    assert_eq!(sides(480., 800.).fitted(px(1320.), OPEN), (px(480.), px(480.)));
    assert_eq!(sides(290., 432.).fitted(px(1000.), OPEN), (px(290.), px(350.)));
    assert_eq!(sides(290., 432.).fitted(px(900.), OPEN), (px(240.), px(300.)), "the right is at its narrowest");

    assert_eq!(sides(290., 432.).fitted(px(600.), OPEN), (px(220.), px(300.)), "neither goes below it");
}

/// What shows of each sidebar, left then right, in a window `across` with each `open` this far.
fn shown(sides: Sides, across: f32, open: (f32, f32)) -> (f32, f32) {
    let (left, right) = sides.fitted(px(across), open);
    (f32::from(left) * open.0, f32::from(right) * open.1)
}

/// A sidebar gives up its room as it slides away, so the other widens as it goes, and neither jumps when the slide
/// sets off or arrives. Going by which was open and which hidden, the other took its whole width in the first frame.
#[test]
fn a_sidebar_sliding_in_a_narrow_window_moves_nothing_by_more_than_it_moves_itself() {
    let steps: Vec<f32> = (0..=100_u8).map(|step| f32::from(step) / 100.).collect();
    for across in [700., 900., 1000., 1081., 1320.] {
        // The left one sliding with the right open, and the right one with the left open.
        for slide in [|open: f32| (open, 1.), |open: f32| (1., open)] {
            let at: Vec<(f32, f32)> = steps.iter().map(|open| shown(sides(290., 432.), across, slide(*open))).collect();
            // The one sliding is the same width all the way, so it slides as one piece.
            let widths: Vec<(Pixels, Pixels)> =
                steps.iter().map(|open| sides(290., 432.).fitted(px(across), slide(*open))).collect();
            let own = |widths: (Pixels, Pixels)| if slide(0.).0 == 0. { widths.0 } else { widths.1 };
            assert!(widths.iter().all(|w| own(*w) == own(widths[0])), "in a window {across} wide: {widths:?}");
            for (pair, open) in at.windows(2).zip(&steps) {
                let moved = (pair[1].0 - pair[0].0).abs().max((pair[1].1 - pair[0].1).abs());
                // A hundredth of the slide moves a sidebar's edge by a hundredth of its width, which is at most 8px.
                assert!(moved <= 8.01, "{moved}px in a step from {open} open, in a window {across} wide");
                let between = across - pair[0].0 - pair[0].1;
                assert!(between >= 359.99 || across < 880., "{between}px between them, in a window {across} wide");
            }
            assert_eq!(at[100], shown(sides(290., 432.), across, OPEN), "and it ends as it is with both open");
        }
    }
}

#[test]
fn a_sidebar_that_isnt_showing_takes_none_of_the_window() {
    // The one on the right hidden, or with no workspace showing to draw it in, and then the one on the left.
    assert_eq!(sides(290., 432.).fitted(px(900.), (1., 0.)).0, px(290.));
    assert_eq!(sides(290., 432.).fitted(px(900.), (0., 1.)).1, px(432.));
}

/// Settings stands in for the sidebar on the left at its width, whether or not it's hidden.
#[test]
fn a_hidden_sidebar_is_as_wide_as_the_window_leaves_it() {
    assert_eq!(sides(480., 432.).fitted(px(700.), (0., 0.)).0, px(340.), "700 less 360 between");
    assert_eq!(sides(480., 432.).fitted(px(1320.), (0., 0.)).0, px(480.));
    assert_eq!(sides(290., 432.).fitted(px(900.), (0., 1.)).0, px(240.), "as it is beside the other");
}

/// How wide `side` comes out when its edge is dragged to `width` in a window `across`, with the other open.
fn dragged_beside(sides: Sides, side: Side, width: f32, across: f32) -> Pixels {
    dragged(side, px(width), px(across) - sides.kept_beside(side))
}

#[test]
fn the_left_sidebar_drags_as_wide_as_leaves_the_right_its_narrowest() {
    let both = sides(290., 432.);

    assert_eq!(dragged_beside(both, Side::Left, 470., 1000.), px(340.), "1000 less 300 for the right and 360 between");
    assert_eq!(
        dragged_beside(both, Side::Right, 700., 1000.),
        px(350.),
        "and the right gives way to the left as it is"
    );
    // With the other hidden, or not drawn, only its own limits hold it.
    let mut alone = both;
    alone.right.sidebar = Sidebar::Hidden;
    assert_eq!(dragged_beside(alone, Side::Left, 470., 1000.), px(470.));
    let mut on_home = both;
    on_home.placed.beside.changes = false;
    assert_eq!(dragged_beside(on_home, Side::Left, 470., 1000.), px(470.));
}

/// The widest a sidebar drags goes by what's chosen for the other, not by how the two are fitted at that moment.
/// Going by the fit, narrowing the left let the right widen into the room, which then held the left where it was.
#[test]
fn an_edge_dragged_in_and_back_out_gets_back_the_width_it_had() {
    let mut both = sides(290., 432.);
    assert_eq!(both.fitted(px(1000.), OPEN), (px(290.), px(350.)));

    both.left.width = dragged_beside(both, Side::Left, 250., 1000.);
    assert_eq!(both.fitted(px(1000.), OPEN), (px(250.), px(390.)), "the right takes the room");
    both.left.width = dragged_beside(both, Side::Left, 290., 1000.);

    assert_eq!(both.fitted(px(1000.), OPEN), (px(290.), px(350.)));
}
