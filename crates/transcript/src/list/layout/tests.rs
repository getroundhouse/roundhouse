use super::*;

mod still;

/// A window 300 tall.
const TALL: Pixels = px(300.);

/// A session whose rows are each as tall as `heights` says, which notes each row it's asked to draw.
struct Session {
    heights: Vec<f32>,
    /// Whether each row's height is known without drawing it.
    known: bool,
    /// The rows whose heights aren't known even so, such as ones that have changed.
    changed: Vec<usize>,
    /// The rows before this one haven't been fetched, and are taken to be 40 tall.
    fetched_from: usize,
    drew: Vec<usize>,
}

impl Session {
    /// One with no height known, so every row laid out is drawn.
    fn of(heights: &[f32]) -> Self {
        Self { heights: heights.to_vec(), known: false, changed: Vec::new(), fetched_from: 0, drew: Vec::new() }
    }

    /// One with every height known, as after its rows have all been measured.
    fn measured(heights: &[f32]) -> Self {
        Self { known: true, ..Self::of(heights) }
    }

    fn from(mut self, start: Start) -> (Layout<()>, Vec<usize>) {
        let layout = lay_out(self.heights.len(), start, TALL, &mut self);
        self.drew.sort_unstable();
        (layout, self.drew)
    }
}

impl Rows<()> for Session {
    fn kept(&self, row: usize) -> Option<Pixels> {
        let known = self.known && row >= self.fetched_from && !self.changed.contains(&row);
        known.then(|| px(self.heights[row]))
    }

    fn draw(&mut self, row: usize) -> Option<(Pixels, ())> {
        self.drew.push(row);
        (row >= self.fetched_from).then(|| (px(self.heights[row]), ()))
    }

    fn guess(&self, _: usize) -> Pixels {
        px(40.)
    }
}

fn top(row: usize, offset: f32) -> Start {
    Start::Top { at: Place { row, offset: px(offset) }, moved: px(0.) }
}

/// The top having moved `moved` down the session from the start of `row`.
fn moved(row: usize, moved: f32) -> Start {
    Start::Top { at: Place { row, offset: px(0.) }, moved: px(moved) }
}

/// Row `row` kept where its top was `top` below the window's, with the top having moved `moved` down the session
/// since.
fn anchored(row: usize, top: f32, moved: f32) -> Start {
    Start::Anchored { at: Anchor { row, top: px(top) }, moved: px(moved) }
}

fn at(row: usize, offset: f32) -> Place {
    Place { row, offset: px(offset) }
}

/// Where each laid row's top is.
fn tops(layout: &Layout<()>) -> Vec<(usize, Pixels)> {
    layout.rows.iter().map(|row| (row.row, row.top)).collect()
}

/// The rows left drawn, which are the ones to show.
fn shown(layout: &Layout<()>) -> Vec<usize> {
    layout.rows.iter().filter(|row| row.drawn.is_some()).map(|row| row.row).collect()
}

#[test]
fn keeping_to_the_latest_row_puts_its_end_at_the_windows_bottom() {
    let (layout, _) = Session::of(&[50.; 100]).from(Start::Latest);

    let last = layout.rows.back().unwrap();
    assert_eq!((last.row, last.bottom()), (99, TALL));
    assert_eq!(layout.top, at(94, 0.));
    assert!(layout.end_in_sight);
    // Above the window too, for as far as rows are laid out past it.
    assert_eq!(layout.rows.front().unwrap().row, 86);
}

#[test]
fn a_session_shorter_than_the_window_sits_at_its_top() {
    for start in [Start::Latest, top(2, 30.), moved(0, -80.)] {
        let (layout, _) = Session::of(&[50., 60., 70.]).from(start);

        assert_eq!(tops(&layout), [(0, px(0.)), (1, px(50.)), (2, px(110.))], "{start:?}");
        assert_eq!(layout.top, Place::default());
        assert!(layout.end_in_sight);
    }
}

#[test]
fn rows_are_laid_from_the_row_at_the_top_to_a_little_past_the_bottom() {
    let (layout, _) = Session::of(&[50.; 100]).from(top(10, 20.));

    assert_eq!(layout.top, at(10, 20.));
    assert!(tops(&layout).contains(&(10, px(-20.))) && tops(&layout).contains(&(11, px(30.))));
    let last = layout.rows.back().unwrap();
    assert!(last.bottom() >= TALL + OVERDRAW && last.top < TALL + OVERDRAW, "{:?}", last.row);
    assert!(!layout.end_in_sight);
}

#[test]
fn a_top_the_wheel_moved_is_found_by_the_rows_it_passed() {
    // The wheel brought 120 px of earlier rows into view: all of rows 9 and 8, and 20 px of row 7.
    let (scrolled_back, _) = Session::of(&[50.; 100]).from(moved(10, -120.));
    let (scrolled_on, _) = Session::of(&[50.; 100]).from(moved(10, 120.));
    // Rows of other heights are passed at the heights they have.
    let uneven = [50., 10., 0., 200., 50., 50., 50., 50., 50., 50., 50., 50.];
    let (uneven, _) = Session::of(&uneven).from(moved(4, -100.));

    assert_eq!(scrolled_back.top, at(7, 30.));
    assert_eq!(scrolled_on.top, at(12, 20.));
    assert_eq!(uneven.top, at(3, 100.));
}

/// Such as a block of thinking that someone closes while the window is part way down it.
#[test]
fn a_place_further_into_its_row_than_the_row_is_now_tall_is_the_rows_end() {
    let mut heights = [50.; 100];
    heights[10] = 30.;

    let (layout, _) = Session::of(&heights).from(top(10, 1500.));
    let (and_moved, _) = Session::of(&heights).from(Start::Top { at: at(10, 1500.), moved: px(-10.) });

    assert_eq!(layout.top, at(11, 0.), "the rows after it aren't passed by as far as it shrank");
    assert_eq!(and_moved.top, at(10, 20.), "and what the wheel has moved since is from there");
}

/// Such as a long text at the top of the window that's parsed once it's there, with the row after it 100 px down the
/// window.
#[test]
fn the_rows_above_the_row_kept_still_grow_up_past_the_windows_top() {
    let mut heights = [50.; 100];
    heights[9] = 1000.;

    let (layout, _) = Session::of(&heights).from(anchored(10, 100., 0.));

    assert_eq!(tops(&layout).iter().find(|(row, _)| *row == 10), Some(&(10, px(100.))));
    assert_eq!(layout.top, at(9, 900.), "as far into the row that grew as it grew");
    assert_eq!(layout.anchor, Some(Anchor { row: 10, top: px(100.) }), "and the same row is kept still next");
}

/// Such as a block someone closes while the window is part way down it, as for a place at the top.
#[test]
fn a_row_kept_still_too_short_to_reach_the_window_from_where_it_was_ends_at_the_windows_top() {
    let mut heights = [50.; 100];
    heights[10] = 30.;

    let (layout, _) = Session::of(&heights).from(anchored(10, -1500., 0.));

    assert_eq!(layout.top, at(11, 0.));
    assert_eq!(layout.anchor, Some(Anchor { row: 11, top: px(0.) }), "the row it ends at is kept still next");
}

#[test]
fn the_row_kept_still_next_is_the_first_to_show_whose_top_is_in_the_window() {
    let mut heights = [50.; 100];
    (heights[11], heights[12]) = (0., 0.);
    let (past_the_top, _) = Session::of(&heights).from(top(10, 20.));
    let (kept, _) = Session::of(&heights).from(anchored(10, -20., 0.));
    let (moved, _) = Session::of(&heights).from(anchored(10, -20., -10.));
    heights[20] = 1000.;
    let (filled, _) = Session::of(&heights).from(top(20, 200.));

    assert_eq!(
        past_the_top.anchor,
        Some(Anchor { row: 13, top: px(30.) }),
        "past the row reaching up, and those of no height"
    );
    assert_eq!(kept.anchor, Some(Anchor { row: 10, top: px(-20.) }), "but the row kept still stays the one");
    assert_eq!(moved.anchor, Some(Anchor { row: 13, top: px(40.) }), "until the list moves, and it's chosen again");
    assert_eq!(
        filled.anchor,
        Some(Anchor { row: 20, top: px(-200.) }),
        "or the row at the top, if it fills the window"
    );
}

#[test]
fn it_stops_at_the_start_of_the_session_and_at_the_end() {
    let (before_the_start, _) = Session::of(&[50.; 100]).from(moved(1, -500.));
    let (past_the_end, _) = Session::of(&[50.; 100]).from(top(98, 0.));

    assert_eq!(before_the_start.top, Place::default());
    assert!(!before_the_start.end_in_sight);
    assert_eq!(past_the_end.top, at(94, 0.));
    assert_eq!(past_the_end.rows.back().unwrap().bottom(), TALL);
    assert!(past_the_end.end_in_sight);
}

#[test]
fn a_row_that_hasnt_been_fetched_takes_its_room_and_draws_nothing() {
    let session = Session { fetched_from: 5, ..Session::of(&[40.; 12]) };

    let (layout, _) = session.from(top(3, 0.));

    assert_eq!(shown(&layout), [5, 6, 7, 8, 9, 10]);
    assert_eq!(tops(&layout).iter().find(|(row, _)| *row == 5), Some(&(5, px(80.))), "below two that take 40 each");
    let unfetched: Vec<usize> = layout.rows.iter().filter(|row| !row.fetched).map(|row| row.row).collect();
    assert_eq!(unfetched, [0, 1, 2, 3, 4]);
}

#[test]
fn rows_of_no_height_are_passed_over() {
    let mut heights = [0.; 60];
    (heights[0], heights[30], heights[59]) = (400., 400., 400.);

    let (layout, _) = Session::of(&heights).from(top(45, 0.));

    assert_eq!(layout.top, at(59, 0.), "the next row with anything to show");
    assert!(!layout.end_in_sight, "which runs on past the window's bottom");
}

#[test]
fn no_rows_lay_out_as_nothing() {
    let (layout, _) = Session::of(&[]).from(Start::Latest);

    assert!(layout.rows.is_empty() && layout.end_in_sight);
}

#[test]
fn only_the_rows_that_show_are_left_drawn() {
    let (layout, drew) = Session::of(&[50.; 100]).from(top(10, 20.));

    // Row 10 from 20 px into it, down to the 20 px of row 16 above the window's bottom.
    assert_eq!(shown(&layout), [10, 11, 12, 13, 14, 15, 16]);
    assert!(drew.len() > 20, "though with no height known, every row laid out was drawn to find its own");
}

#[test]
fn a_row_out_of_the_window_whose_height_is_known_isnt_drawn() {
    let (layout, drew) = Session::measured(&[50.; 100]).from(top(10, 20.));
    let (following, drew_following) = Session::measured(&[50.; 100]).from(Start::Latest);

    assert_eq!(drew, [10, 11, 12, 13, 14, 15, 16], "each row that shows, once");
    assert_eq!(shown(&layout), drew);
    assert_eq!(layout.rows.front().unwrap().row, 2, "the rows past the window's ends have their places all the same");
    assert_eq!(drew_following, [94, 95, 96, 97, 98, 99]);
    assert_eq!(shown(&following), drew_following);
}

/// Most of a session's rows show nothing, such as the events of a call that its first row draws, and a run of
/// hundreds of them fits in a window.
#[test]
fn a_row_known_to_show_nothing_isnt_drawn_wherever_it_is() {
    let mut heights = [0.; 500];
    (heights[0], heights[250], heights[499]) = (100., 100., 100.);

    let (layout, drew) = Session::measured(&heights).from(top(0, 0.));

    assert_eq!(drew, [0, 250, 499]);
    assert_eq!(shown(&layout), drew);
}

#[test]
fn a_row_whose_height_isnt_known_is_drawn_to_find_it_and_shown_only_in_the_window() {
    // One in the window and one a little below it have changed.
    let session = Session { changed: vec![12, 20], ..Session::measured(&[50.; 100]) };

    let (layout, drew) = session.from(top(10, 0.));

    assert_eq!(drew, [10, 11, 12, 13, 14, 15, 20]);
    assert_eq!(shown(&layout), [10, 11, 12, 13, 14, 15]);
}

/// The rows are given their places from the top, stop short of the window's bottom at the end of the session, and
/// all move down. Those that were above the window when they were placed are in it now.
#[test]
fn rows_that_move_into_the_window_at_an_end_of_the_session_are_drawn() {
    let (at_the_end, drew_at_the_end) = Session::measured(&[50.; 100]).from(top(98, 0.));
    let (at_the_start, drew_at_the_start) = Session::measured(&[50.; 100]).from(moved(20, -5000.));

    assert_eq!(shown(&at_the_end), [94, 95, 96, 97, 98, 99]);
    assert_eq!(drew_at_the_end, [94, 95, 96, 97, 98, 99], "each once");
    assert_eq!(shown(&at_the_start), [0, 1, 2, 3, 4, 5]);
    assert_eq!(drew_at_the_start, [0, 1, 2, 3, 4, 5]);
}

/// Such as a prompt jumped to from the outline, with the reply it answered above it.
#[test]
fn a_row_sent_to_the_bottom_ends_there_with_the_rows_before_it_above() {
    let (layout, _) = Session::of(&[50.; 100]).from(Start::AtBottom { row: 40 });
    let unfetched = Session { fetched_from: 50, ..Session::of(&[50.; 100]) };
    let (unfetched, _) = unfetched.from(Start::AtBottom { row: 40 });

    let ends = |layout: &Layout<()>| layout.rows.iter().find(|row| row.row == 40).map(LaidRow::bottom);
    assert_eq!(ends(&layout), Some(TALL));
    assert_eq!(layout.top, at(35, 0.));
    assert_eq!(layout.last_in_sight, 40, "it's the last row in the window");
    assert!(!layout.end_in_sight);
    assert_eq!(ends(&unfetched), Some(TALL), "at the height it's guessed to be, until it's fetched");
    assert_eq!(unfetched.last_in_sight, 40);
}

#[test]
fn a_row_sent_to_the_bottom_that_is_taller_than_the_window_shows_from_its_start() {
    let mut heights = [50.; 100];
    heights[40] = 500.;

    let (layout, _) = Session::of(&heights).from(Start::AtBottom { row: 40 });

    assert_eq!(layout.top, at(40, 0.));
    assert_eq!(layout.last_in_sight, 40);
}

#[test]
fn a_row_sent_to_the_bottom_near_the_start_stays_where_it_is_with_the_first_row_at_the_top() {
    let (layout, _) = Session::of(&[50.; 100]).from(Start::AtBottom { row: 2 });

    assert_eq!(layout.top, Place::default());
    assert_eq!(tops(&layout).iter().find(|(row, _)| *row == 2), Some(&(2, px(100.))));
    assert_eq!(layout.last_in_sight, 5, "and the rows below it fill the window");
}

#[test]
fn the_bottom_row_is_the_last_with_more_than_a_pixel_in_the_window() {
    // Row 16 starts 20 px above the window's bottom, and then half a pixel above it.
    let (layout, _) = Session::of(&[50.; 100]).from(top(10, 20.));
    let (nearly, _) = Session::of(&[50.; 100]).from(top(10, 0.5));

    assert_eq!(layout.last_in_sight, 16);
    assert_eq!(nearly.last_in_sight, 15);
}
