use image::{Rgba, RgbaImage};

use super::{CELL, Changes, Place, joined, uncovered};

const WHITE: Rgba<u8> = Rgba([255, 255, 255, 255]);
const YELLOW: Rgba<u8> = Rgba([230, 190, 60, 255]);

fn white(width: u32, height: u32) -> RgbaImage {
    RgbaImage::from_pixel(width, height, WHITE)
}

/// `image` with each of `pixels` a shade off white.
fn with_changes(mut image: RgbaImage, pixels: &[(u32, u32)]) -> RgbaImage {
    for (x, y) in pixels {
        image.put_pixel(*x, *y, Rgba([255, 255, 254, 255]));
    }
    image
}

#[test]
fn the_same_image_has_nothing_changed_and_is_drawn_faded() {
    let image = white(40, 30);

    let changes = Changes::between(&image, &image);

    assert_eq!((changes.count(), changes.places()), (0, Vec::new()));
    assert_eq!(*changes.picture(&image, &image, YELLOW).get_pixel(3, 4), Rgba([169, 169, 169, 255]));
}

/// The least difference there is counts, as it does for the gallery.
#[test]
fn a_pixel_a_shade_off_is_a_change_tinted_its_own_colour() {
    let (before, after) = (white(40, 30), with_changes(white(40, 30), &[(25, 7)]));

    let changes = Changes::between(&before, &after);

    assert_eq!(changes.count(), 1);
    assert_eq!(changes.places(), [Place { x: 25, y: 7, width: 1, height: 1 }]);
    // Mostly yellow, with the rest the white that's there.
    assert_eq!(*changes.picture(&before, &after, YELLOW).get_pixel(25, 7), Rgba([240, 216, 138, 255]));
}

/// By the boxes around them, wherever they fall in the squares changes are first gathered in.
#[test]
fn changes_up_to_a_cell_apart_are_one_place_and_farther_two() {
    let before = white(200, 100);
    // With as many pixels as a cell between them that haven't changed, and with one more.
    let close = with_changes(white(200, 100), &[(2, 2), (3 + CELL, 9)]);
    let apart = with_changes(white(200, 100), &[(2, 2), (4 + CELL, 9)]);
    let far = with_changes(white(200, 100), &[(2, 2), (150, 80)]);

    let changes = |after: &RgbaImage| Changes::between(&before, after).places();

    assert_eq!(changes(&close), [Place { x: 2, y: 2, width: CELL + 2, height: 8 }]);
    let two = [Place { x: 2, y: 2, width: 1, height: 1 }, Place { x: 4 + CELL, y: 9, width: 1, height: 1 }];
    assert_eq!(changes(&apart), two);
    assert_eq!(
        changes(&far),
        [Place { x: 2, y: 2, width: 1, height: 1 }, Place { x: 150, y: 80, width: 1, height: 1 }]
    );
}

/// A box inside another, across it or against it would be read as a change inside a change, or the two as one box.
#[test]
fn places_whose_boxes_meet_or_nearly_are_one() {
    let edges: Vec<_> = (0..100).flat_map(|at| [(at, 0), (0, at)]).collect();
    let inside = with_changes(white(100, 100), &[edges, vec![(60, 60)]].concat());
    // In squares with one between them, and no more than one apart.
    let near = with_changes(white(100, 100), &[(CELL - 1, 50), (2 * CELL, 50)]);

    let (one, other) = (Changes::between(&white(100, 100), &inside), Changes::between(&white(100, 100), &near));

    assert_eq!(one.places(), [Place { x: 0, y: 0, width: 100, height: 100 }]);
    assert_eq!(other.places(), [Place { x: CELL - 1, y: 50, width: CELL + 2, height: 1 }]);
}

/// From their top left, so what only the wider one covers is set apart, and only what both cover is compared.
#[test]
fn what_only_one_image_covers_is_set_apart_and_not_compared() {
    let (before, after) = (white(10, 10), with_changes(white(12, 10), &[(3, 3), (11, 4)]));

    let changes = Changes::between(&before, &after);

    assert_eq!((changes.width, changes.height, changes.count()), (12, 10, 1));
    assert_eq!(changes.places(), [Place { x: 3, y: 3, width: 1, height: 1 }]);
    assert_eq!(uncovered([(10, 10), (12, 10)]), [Place { x: 10, y: 0, width: 2, height: 10 }]);
}

/// One wider and the other taller: each has what's to the right of or below what both cover.
#[test]
fn what_only_one_covers_is_found_whichever_way_each_is_bigger() {
    let [wider, taller] = [(100, 50), (50, 100)];

    assert_eq!(
        uncovered([wider, taller]),
        [Place { x: 50, y: 0, width: 50, height: 50 }, Place { x: 0, y: 50, width: 50, height: 50 }]
    );
    assert_eq!(uncovered([wider, wider]), []);
}

/// Outlines stand off their places on the screen, which is more of the image the smaller it's drawn, so places as far
/// apart as that are drawn as one.
#[test]
fn places_drawn_small_are_joined_as_far_apart_as_their_outlines_would_meet() {
    let changes = Changes::between(&white(200, 100), &with_changes(white(200, 100), &[(2, 2), (40, 2), (150, 80)]));
    let places = changes.places();
    assert_eq!(places.len(), 3, "far enough apart to count as three");

    let drawn = joined(places, 40);

    assert_eq!(drawn, [Place { x: 2, y: 2, width: 39, height: 1 }, Place { x: 150, y: 80, width: 1, height: 1 }]);
}
