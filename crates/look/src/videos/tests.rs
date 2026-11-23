use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, Modifiers, ParentElement as _, Render, Styled as _, TestAppContext,
    VisualTestContext, Window, div, point, px,
};

use gpui_kit::test::TestQueryExt as _;

use super::{VideoShows, length, video};

/// Minutes and seconds, in whole seconds, as a player counts them, and hours once there are any.
#[test]
fn a_length_reads_as_a_player_says_it() {
    assert_eq!(length(Duration::ZERO), "0:00");
    assert_eq!(length(Duration::from_millis(7200)), "0:07");
    assert_eq!(length(Duration::from_secs(750)), "12:30");
    assert_eq!(length(Duration::from_secs(3723)), "1:02:03");
}

/// A video 400 px wide, of four seconds, showing `shows`, which notes each seek it's asked for, with Expand if it
/// `expands`, a line over its picture if it `says` one, and something drawn over the whole of it if it's `covered`, as
/// a dialog is.
struct Harness {
    shows: VideoShows,
    expands: bool,
    says: Option<&'static str>,
    covered: bool,
    seeks: Rc<RefCell<Vec<f32>>>,
}

impl Render for Harness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let seeks = self.seeks.clone();
        let picture = div().w_full().h(px(200.)).into_any_element();
        let mut drawn = video("video", picture, self.shows, Some(Duration::from_secs(4)))
            .on_play(|_, _| {})
            .on_pause(|_, _| {})
            .on_seek(move |along, _, _| seeks.borrow_mut().push(along));
        if self.expands {
            drawn = drawn.on_expand(|_, _| {});
        }
        if let Some(says) = self.says {
            drawn = drawn.says(says);
        }
        let over = self.covered.then(|| div().id("over").absolute().inset_0().occlude());
        div().relative().w(px(400.)).child(drawn).children(over)
    }
}

fn drawn(cx: &mut TestAppContext, made: Harness) -> &mut VisualTestContext {
    cx.update(gpui_kit::init);
    cx.update(crate::theme::apply);
    let (_, cx) = cx.add_window_view(move |_, _| made);
    cx.run_until_parked();
    cx
}

fn harness(shows: VideoShows) -> Harness {
    Harness { shows, expands: true, says: None, covered: false, seeks: Rc::default() }
}

/// How many elements are labelled `label`.
fn labelled(cx: &mut VisualTestContext, label: &str) -> usize {
    cx.update(|window, _| window.find_all_by_label(label).len())
}

/// A press along the track goes as far along the video.
#[gpui_kit::test]
fn a_press_along_the_track_goes_as_far_along_the_video(cx: &mut TestAppContext) {
    let made = harness(VideoShows::Paused(Duration::from_secs(1)));
    let seeks = made.seeks.clone();
    let cx = drawn(cx, made);
    let track = cx.update(|window, _| window.find_all_by_label("Position").remove(0).bounds());

    let quarter = point(track.origin.x + track.size.width / 4., track.center().y);
    cx.simulate_click(quarter, Modifiers::default());

    let seeks = seeks.borrow();
    assert_eq!(seeks.len(), 1, "{seeks:?}");
    assert!((seeks[0] - 0.25).abs() < 0.01, "a quarter of the way: {seeks:?}");
}

/// A press on what's drawn over the track, such as a dialog, isn't the track's.
#[gpui_kit::test]
fn a_press_on_what_s_drawn_over_the_track_doesn_t_go_along_it(cx: &mut TestAppContext) {
    let made = Harness { covered: true, ..harness(VideoShows::Paused(Duration::from_secs(1))) };
    let seeks = made.seeks.clone();
    let cx = drawn(cx, made);
    let track = cx.update(|window, _| window.find_all_by_label("Position").remove(0).bounds());

    cx.simulate_click(track.center(), Modifiers::default());

    assert!(seeks.borrow().is_empty());
}

/// The play mark is over the still only, Play in the bar turns to Pause while it plays, and Expand is only there for
/// a video that isn't already over the window.
#[gpui_kit::test]
fn the_play_mark_is_over_the_still_and_expand_only_where_there_s_more_room(cx: &mut TestAppContext) {
    let still = drawn(cx, harness(VideoShows::Still));
    assert_eq!(labelled(still, "Play"), 2, "the mark and the bar's");
    assert_eq!(labelled(still, "Expand"), 1);

    let playing = drawn(cx, Harness { expands: false, ..harness(VideoShows::Playing(Duration::from_secs(1))) });
    assert_eq!(labelled(playing, "Play"), 0);
    assert_eq!(labelled(playing, "Pause"), 1);
    assert_eq!(labelled(playing, "Expand"), 0, "already over the window");
    assert_eq!(labelled(playing, "0:01 / 0:04"), 1, "where it is of how long");
}

/// A line over the picture, such as why it couldn't be played, takes the play mark's place.
#[gpui_kit::test]
fn a_line_over_the_picture_takes_the_play_mark_s_place(cx: &mut TestAppContext) {
    let cx = drawn(cx, Harness { says: Some("The video couldn't be played"), ..harness(VideoShows::Still) });

    assert_eq!(labelled(cx, "Play"), 1, "only the bar's");
}
