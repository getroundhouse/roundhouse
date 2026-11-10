use super::*;

fn display(number: u64, uuid: &str, width: f32, height: f32) -> Attached {
    Attached {
        id: DisplayId::new(number),
        uuid: Some(DisplayUuid(uuid.to_string())),
        bounds: Bounds::new(point(px(0.), px(0.)), size(px(width), px(height))),
        main: number == 1,
    }
}

/// A laptop's own display, and a larger one beside it.
fn displays() -> Vec<Attached> {
    vec![display(1, "built-in", 1512., 982.), display(2, "desk", 2560., 1440.)]
}

fn place(display: &str, (x, y): (f32, f32), (width, height): (f32, f32), mode: Mode) -> Place {
    Place { display: Some(DisplayUuid(display.to_string())), x, y, width, height, mode }
}

fn bounds((x, y): (f32, f32), (width, height): (f32, f32)) -> Bounds<Pixels> {
    Bounds::new(point(px(x), px(y)), size(px(width), px(height)))
}

#[test]
fn a_window_opens_where_it_was_on_the_display_it_was_on() {
    let left = place("desk", (40., 60.), (1800., 1200.), Mode::Windowed);

    let opening = left.opening(&displays()).unwrap();

    assert_eq!(opening.display, DisplayId::new(2));
    assert_eq!(opening.bounds, WindowBounds::Windowed(bounds((40., 60.), (1800., 1200.))));
}

#[test]
fn each_mode_opens_as_itself_with_where_it_goes_back_to() {
    let full_screen = place("built-in", (96., 61.), (1320., 860.), Mode::FullScreen);
    let maximized = place("built-in", (96., 61.), (1320., 860.), Mode::Maximized);
    let back_to = bounds((96., 61.), (1320., 860.));

    assert_eq!(full_screen.opening(&displays()).unwrap().bounds, WindowBounds::Fullscreen(back_to));
    assert_eq!(maximized.opening(&displays()).unwrap().bounds, WindowBounds::Maximized(back_to));
}

#[test]
fn a_window_whose_display_is_gone_opens_in_its_usual_place() {
    let left = place("at the office", (40., 60.), (1800., 1200.), Mode::Windowed);

    assert_eq!(left.opening(&displays()), None);
}

#[test]
fn a_window_on_a_display_with_no_id_opens_on_the_main_one() {
    let left = Place { display: None, ..place("", (40., 60.), (1200., 800.), Mode::Windowed) };

    assert_eq!(left.opening(&displays()).unwrap().display, DisplayId::new(1));
}

#[test]
fn a_window_partly_off_its_display_opens_where_it_was() {
    let left = place("built-in", (1400., 900.), (1320., 860.), Mode::Windowed);

    assert!(left.opening(&displays()).is_some());
}

#[test]
fn a_window_out_of_reach_opens_in_its_usual_place() {
    // As when the display is set to show less than it did.
    let off_the_side = place("built-in", (1500., 60.), (1320., 860.), Mode::Windowed);
    let off_the_bottom = place("built-in", (40., 970.), (1320., 860.), Mode::Windowed);
    let nowhere_near = place("built-in", (4000., 60.), (1320., 860.), Mode::Windowed);
    // Plenty of it is on the display, but not the strip along its top, which it's moved by.
    let off_the_top = place("built-in", (40., -800.), (1320., 860.), Mode::Windowed);

    assert_eq!(off_the_side.opening(&displays()), None);
    assert_eq!(off_the_bottom.opening(&displays()), None);
    assert_eq!(nowhere_near.opening(&displays()), None);
    assert_eq!(off_the_top.opening(&displays()), None);
}

#[test]
fn a_size_that_isnt_one_opens_in_its_usual_place() {
    let not_sizes =
        [(0., 860.), (1320., 0.), (-1320., 860.), (1320., -860.), (f32::NAN, 860.), (1320., f32::NAN), (1320., 12.)];
    for (width, height) in not_sizes {
        let left = place("built-in", (40., 60.), (width, height), Mode::Windowed);

        assert_eq!(left.opening(&displays()), None, "{width} by {height}");
    }
    let nowhere = place("built-in", (f32::NAN, 60.), (1320., 860.), Mode::Windowed);
    assert_eq!(nowhere.opening(&displays()), None);
}

#[test]
fn a_window_still_in_its_usual_place_isnt_saved_and_one_that_moved_is() {
    let usual = place("built-in", (96., 61.), (1320., 860.), Mode::Windowed);
    let left_on_a_display_now_gone = place("desk", (40., 60.), (1800., 1200.), Mode::Windowed);
    let moved = place("built-in", (0., 33.), (1512., 949.), Mode::Windowed);

    assert!(!usual.is_news(None, Some(&usual)));
    assert!(!usual.is_news(Some(&left_on_a_display_now_gone), Some(&usual)));
    assert!(moved.is_news(Some(&left_on_a_display_now_gone), Some(&usual)));
    assert!(!moved.is_news(Some(&moved), None));
}

#[test]
fn leaving_full_screen_is_saved_though_the_window_is_back_where_it_opened() {
    // A window left in full screen opens where it goes back to, then goes into full screen.
    let in_full_screen = place("built-in", (0., 33.), (1512., 949.), Mode::FullScreen);
    let back_out = Place { mode: Mode::Windowed, ..in_full_screen.clone() };

    assert!(back_out.is_news(Some(&in_full_screen), None));
}

#[test]
fn what_is_saved_reads_back_the_same() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data").join(FILE);
    let left = place("desk", (40.5, 60.), (1800., 1200.), Mode::FullScreen);

    write_json(&path, &left).unwrap();

    assert_eq!(read(&path), Some(left));
}

#[test]
fn nothing_saved_and_a_file_that_cant_be_read_both_count_as_nowhere() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE);

    assert_eq!(read(&path), None);
    std::fs::write(&path, "{ not json").unwrap();
    assert_eq!(read(&path), None);
    std::fs::write(&path, r#"{"display": null, "x": 1, "y": 2, "width": 3, "height": 4, "mode": "tiled"}"#).unwrap();
    assert_eq!(read(&path), None, "a mode this build doesn't know");
}
