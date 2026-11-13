use gpui_kit::{Pixels, px};

use super::super::{Line, places};
use super::{Fold, Laid, folds, lay, rests};
use crate::reorder::Arrangement;

/// Two projects under the heading, the first with two workspaces and the second with one.
const LINES: [Line; 6] =
    [Line::Heading, Line::Repo(0), Line::Workspace(0), Line::Workspace(1), Line::Repo(1), Line::Workspace(2)];

/// How far the second project's row is from the heading's top with the first project's workspaces showing, and how
/// much nearer with the first collapsed: their two rows and the space under each, less what the space under their
/// project's row grows by when the next project comes straight after it.
const SECOND_OPEN: f32 = 154.;
const FOLDS_BY: f32 = 82.;

/// What [`Places`](crate::reorder::Places) sends lines whose places are `places`, each `off` from its place in this
/// frame, and on its way there unless it's there.
fn sent(places: &[Pixels], off: &[f32]) -> Arrangement {
    Arrangement {
        places: places.into(),
        drawn: places.iter().zip(off).map(|(&place, &off)| place + px(off)).collect(),
        moving: off.iter().map(|off| off.abs() > 0.).collect(),
        end: places[places.len() - 1],
    }
}

/// `LINES` laid out with the first project's workspaces as `first` says, and what comes after them `moved` below its
/// place, which is above it for less than none.
fn laid(first: Fold, moved: f32) -> Laid {
    let folds = folds(&LINES, |repo| if repo == 0 { first } else { Fold::Open });
    let resting: Vec<Line> = (LINES.iter().zip(&folds)).filter(|&(&l, &f)| rests(l, f)).map(|(&l, _)| l).collect();
    let places = places(&resting);
    let after_first = resting.iter().position(|line| matches!(line, Line::Repo(1))).unwrap();
    let off: Vec<f32> = (0..places.len()).map(|at| if at < after_first { 0. } else { moved }).collect();
    lay(&LINES, &folds, &sent(&places, &off))
}

/// How far down the list each line is drawn.
fn drawn(laid: &Laid) -> Vec<Pixels> {
    laid.lines.iter().map(|line| line.top).collect()
}

/// How much of each of the first project's two rows shows.
fn cut(laid: &Laid) -> [Option<Pixels>; 2] {
    [laid.lines[2].cut, laid.lines[3].cut]
}

/// How far out the first project's row says its workspaces are.
fn out(laid: &Laid) -> f32 {
    laid.lines[1].out
}

#[test]
fn at_rest_each_line_is_drawn_whole_at_its_place() {
    let laid = laid(Fold::Open, 0.);

    assert_eq!(drawn(&laid), [0., 32., 70., 110., SECOND_OPEN, SECOND_OPEN + 38.].map(px));
    assert_eq!(laid.end, px(SECOND_OPEN + 74.));
    assert!(laid.lines.iter().all(|line| line.cut.is_none()));
    assert!((out(&laid) - 1.).abs() < f32::EPSILON);
    assert!(laid.settled.is_empty());
}

#[test]
fn a_projects_workspaces_stay_where_they_are_and_are_cut_off_at_what_slides_up_over_them() {
    let halfway = laid(Fold::Closing, FOLDS_BY / 2.);

    let second = SECOND_OPEN - FOLDS_BY / 2.;
    assert_eq!(drawn(&halfway), [0., 32., 70., 110., second, second + 38.].map(px), "the rows haven't moved");
    assert_eq!(cut(&halfway), [Some(px(35.)), Some(px(0.))], "down to the space kept above the next project");
    assert_eq!(halfway.end, px(second + 74.), "and the list ends where its last line does");
    assert!((out(&halfway) - 35. / 76.).abs() < 0.001, "the chevron has turned as far as the rows show");
    assert!(halfway.settled.is_empty(), "it's still on its way");
}

#[test]
fn a_fold_is_over_once_what_comes_after_is_in_its_place_with_every_row_covered() {
    let away = laid(Fold::Closing, 0.);

    assert_eq!(away.settled, [0]);
    assert_eq!(cut(&away), [Some(px(0.)), Some(px(0.))]);
    assert_eq!(drawn(&away)[4], px(SECOND_OPEN - FOLDS_BY));
    assert!(out(&away).abs() < f32::EPSILON);
}

#[test]
fn a_project_opening_shows_its_workspaces_as_what_comes_after_slides_down_off_them() {
    let (starting, halfway) = (laid(Fold::Opening, -FOLDS_BY), laid(Fold::Opening, -FOLDS_BY / 2.));
    let open = laid(Fold::Opening, 0.);

    assert_eq!(cut(&starting), [Some(px(0.)), Some(px(0.))], "nothing shows as it sets off");
    assert_eq!(drawn(&starting)[4], px(SECOND_OPEN - FOLDS_BY), "since what comes after hasn't moved yet");
    assert_eq!(cut(&halfway), [Some(px(35.)), Some(px(0.))]);
    assert!(starting.settled.is_empty() && halfway.settled.is_empty());
    assert_eq!(cut(&open), [Some(px(36.)), Some(px(36.))], "every row shows whole once it's there");
    assert_eq!(drawn(&open), drawn(&laid(Fold::Open, 0.)));
    assert_eq!(open.settled, [0]);
}

#[test]
fn the_last_projects_workspaces_fold_away_under_the_lists_end() {
    let lines = [Line::Repo(0), Line::Workspace(0), Line::Workspace(1)];
    let folds = folds(&lines, |_| Fold::Closing);
    // Its row is all that takes room at rest, and the end has half of the two rows' 76px still to come up.
    let laid = lay(&lines, &folds, &sent(&[px(0.), px(32.)], &[0., 44.]));

    let cuts: Vec<_> = laid.lines.iter().map(|line| line.cut).collect();
    assert_eq!(cuts, [None, Some(px(36.)), Some(px(0.))], "with no space kept above the end");
    assert_eq!(laid.end, px(76.), "the list ends where the rows are cut off");
}

#[test]
fn lines_changing_places_are_drawn_where_theyve_got_to() {
    let lines = [Line::Repo(0), Line::Workspace(0), Line::Workspace(1)];
    let folds = folds(&lines, |_| Fold::Open);
    // The two rows have just changed places, so each is still where the other goes.
    let laid = lay(&lines, &folds, &sent(&[0., 38., 78., 114.].map(px), &[0., 40., -40., 0.]));

    assert_eq!(drawn(&laid), [0., 78., 38.].map(px));
    assert_eq!(laid.end, px(114.));
    assert!(laid.lines.iter().all(|line| line.cut.is_none()), "and nothing is cut off, since nothing folds");
}

/// What comes after a fold can be on a clock of its own, as when a project above it was folded a moment before, so
/// either can get to its place before the other.
#[test]
fn a_fold_isnt_over_until_both_its_projects_row_and_what_comes_after_are_in_their_places() {
    let folds = folds(&LINES, |repo| if repo == 0 { Fold::Closing } else { Fold::Open });
    let places = [0., 32., 72., 110., 146.].map(px);

    let row_still_moving = lay(&LINES, &folds, &sent(&places, &[0., 20., 0., 0., 0.]));
    let after_still_moving = lay(&LINES, &folds, &sent(&places, &[0., 0., 20., 20., 20.]));

    assert!(row_still_moving.settled.is_empty(), "the rows go by the project's row, so they'd jump");
    assert!(after_still_moving.settled.is_empty(), "the rows are cut off at what comes after");
    assert_eq!(lay(&LINES, &folds, &sent(&places, &[0.; 5])).settled, [0], "and once both are, it's over");
}
