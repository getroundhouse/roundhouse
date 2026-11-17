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

/// A window left windowed on the laptop's own display.
fn on_the_laptop(x_y: (f32, f32), size: (f32, f32)) -> Place {
    place("built-in", x_y, size, Mode::Windowed)
}

/// Where a window opens: on which display and as what, or `None` for its usual place.
type Opens = Option<(DisplayId, WindowBounds)>;

/// Opening windowed on display `number`.
fn windowed(number: u64, x_y: (f32, f32), size: (f32, f32)) -> (DisplayId, WindowBounds) {
    (DisplayId::new(number), WindowBounds::Windowed(bounds(x_y, size)))
}

/// Where each window left somewhere opens.
fn openings() -> Vec<(String, Place, Opens)> {
    let (x_y, size) = ((96., 61.), (1320., 860.));
    let opened_as = |mode| Some((DisplayId::new(1), mode));
    let named = [
        (
            "where it was",
            place("desk", (40., 60.), (1800., 1200.), Mode::Windowed),
            Some(windowed(2, (40., 60.), (1800., 1200.))),
        ),
        // Each mode opens as itself, with where it goes back to.
        (
            "in full screen",
            place("built-in", x_y, size, Mode::FullScreen),
            opened_as(WindowBounds::Fullscreen(bounds(x_y, size))),
        ),
        (
            "maximized",
            place("built-in", x_y, size, Mode::Maximized),
            opened_as(WindowBounds::Maximized(bounds(x_y, size))),
        ),
        (
            "on the main display, with no id",
            Place { display: None, ..on_the_laptop((40., 60.), (1200., 800.)) },
            Some(windowed(1, (40., 60.), (1200., 800.))),
        ),
        ("partly off its display", on_the_laptop((1400., 900.), size), Some(windowed(1, (1400., 900.), size))),
        ("on a display that's gone", place("at the office", (40., 60.), (1800., 1200.), Mode::Windowed), None),
        // Out of reach, as when the display is set to show less than it did.
        ("off the side", on_the_laptop((1500., 60.), size), None),
        ("off the bottom", on_the_laptop((40., 970.), size), None),
        ("nowhere near", on_the_laptop((4000., 60.), size), None),
        // Plenty of it is on the display, but not the strip along its top, which it's moved by.
        ("off the top", on_the_laptop((40., -800.), size), None),
        ("at a place that isn't one", on_the_laptop((f32::NAN, 60.), size), None),
    ];
    let not_sizes =
        [(0., 860.), (1320., 0.), (-1320., 860.), (1320., -860.), (f32::NAN, 860.), (1320., f32::NAN), (1320., 12.)];
    let not_a_size =
        |(width, height)| (format!("{width} by {height}"), on_the_laptop((40., 60.), (width, height)), None);
    let named = named.into_iter().map(|(name, left, opens)| (name.to_string(), left, opens));
    named.chain(not_sizes.into_iter().map(not_a_size)).collect()
}

/// A window opens where it was left, on the display it was on, unless that display is gone, or too little of the
/// window would be on it to reach, or what was saved isn't a place.
#[test]
fn a_window_opens_where_it_was_left_unless_it_would_be_out_of_reach() {
    for (name, left, want) in openings() {
        let opening = left.opening(&displays()).map(|opening| (opening.display, opening.bounds));

        assert_eq!(opening, want, "{name}");
    }
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
