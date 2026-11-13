use std::sync::Arc;

use super::*;

const SIZE: Size = Size { cols: 20, rows: 4 };
const RED: Rgb = Rgb { r: 255, g: 0, b: 0 };
const BLUE: Rgb = Rgb { r: 0, g: 0, b: 255 };

fn mirror_of(written: &[u8]) -> Mirror {
    let mut mirror = Mirror::new(SIZE).unwrap();
    mirror.write(written);
    mirror
}

fn glyph(col: u16, text: char, color: Rgb, look: Look) -> Glyph {
    Glyph { col, text: GlyphText::Char(text), color, look, wide: false }
}

#[test]
fn a_frame_has_each_cells_text_colour_and_style() {
    let mut mirror = mirror_of(b"a\x1b[1;38;2;255;0;0mb\x1b[0m\x1b[48;2;0;0;255m c\x1b[0m \x1b[4m \x1b[0m");

    let frame = mirror.frame().unwrap();

    let plain = frame.foreground;
    let bold = Look { bold: true, ..Look::default() };
    let underline = Look { underline: true, ..Look::default() };
    let row = &frame.rows[0];
    assert_eq!(
        row.glyphs,
        [
            glyph(0, 'a', plain, Look::default()),
            glyph(1, 'b', RED, bold),
            glyph(3, 'c', plain, Look::default()),
            glyph(5, ' ', plain, underline)
        ],
        "a blank cell draws nothing unless it's underlined"
    );
    assert_eq!(row.backgrounds, [Background { col: 2, cells: 2, color: BLUE }], "one stretch for the run");
    assert_eq!(frame.size, SIZE);
    assert_eq!(frame.rows.len(), 4);
    assert!(frame.rows[1].glyphs.is_empty() && frame.rows[1].backgrounds.is_empty());
}

#[test]
fn inverse_swaps_a_cells_colours() {
    let mut mirror = mirror_of(b"\x1b[7;38;2;255;0;0mx");

    let frame = mirror.frame().unwrap();

    assert_eq!(frame.rows[0].backgrounds, [Background { col: 0, cells: 1, color: RED }]);
    assert_eq!(frame.rows[0].glyphs, [glyph(0, 'x', frame.background, Look::default())]);
}

/// A cluster is a character with the marks that combine with it. An emoji with a skin tone takes two cells for each, as
/// a shell's programs count it, since clustering is off unless a program turns it on.
#[test]
fn a_wide_character_takes_two_cells_and_a_cluster_draws_as_one() {
    let mut mirror = mirror_of("日本e\u{301}a".as_bytes());

    let frame = mirror.frame().unwrap();

    let texts: Vec<_> = frame.rows[0].glyphs.iter().map(|g| (g.col, g.text.clone(), g.wide)).collect();
    assert_eq!(
        texts,
        [
            (0, GlyphText::Char('日'), true),
            (2, GlyphText::Char('本'), true),
            (4, GlyphText::Cluster("e\u{301}".into()), false),
            (5, GlyphText::Char('a'), false),
        ]
    );
}

/// A wide character's second cell is part of it, so only the cells that draw nothing between are blanks.
#[test]
fn a_rows_text_has_a_blank_for_each_cell_that_draws_nothing() {
    let mut mirror = mirror_of("日 e\u{301}  a".as_bytes());

    let frame = mirror.frame().unwrap();

    assert_eq!(frame.rows[0].text(), "日 e\u{301}  a");
    assert_eq!(frame.rows[1].text(), "");
}

/// As a program that paints its background does, such as htop or tmux's status line, since the terminal says it can.
#[test]
fn cells_cleared_in_a_colour_have_its_background() {
    let mut mirror = mirror_of(b"\x1b[48;2;0;0;255m\x1b[K");

    let frame = mirror.frame().unwrap();

    assert_eq!(frame.rows[0].backgrounds, [Background { col: 0, cells: SIZE.cols, color: BLUE }]);
}

/// On either of its cells, so it doesn't hide half of it.
#[test]
fn a_cursor_on_a_wide_character_covers_both_its_cells() {
    let cursor = |written: &[u8]| {
        let cursor = mirror_of(written).frame().unwrap().cursor.unwrap();
        (cursor.col, cursor.wide)
    };

    assert_eq!(cursor("日\x1b[1G".as_bytes()), (0, true), "on its first cell");
    assert_eq!(cursor("日\x1b[2G".as_bytes()), (0, true), "on its second");
    assert_eq!(cursor("日a\x1b[3G".as_bytes()), (2, false), "on the next");
}

/// So a view can tell which rows to draw again, and a frame copies only those.
#[test]
fn a_row_that_hasnt_changed_is_the_same_as_in_the_frame_before() {
    let mut mirror = mirror_of(b"one\r\ntwo");
    let before = mirror.frame().unwrap();

    mirror.write(b"!");
    let after = mirror.frame().unwrap();

    assert!(Arc::ptr_eq(&before.rows[0], &after.rows[0]));
    assert!(!Arc::ptr_eq(&before.rows[1], &after.rows[1]));
    assert_eq!(after.rows[1].glyphs.len(), 4);
}

#[test]
fn the_cursor_is_where_the_program_left_it_in_the_shape_it_set() {
    let mut mirror = mirror_of(b"ab\r\n\x1b[5 qc");

    let cursor = mirror.frame().unwrap().cursor.unwrap();
    mirror.write(b"\x1b[?25l");
    let hidden = mirror.frame().unwrap().cursor;

    assert_eq!((cursor.col, cursor.row, cursor.shape), (1, 1, CursorShape::Bar));
    assert_eq!(hidden, None);
}

#[test]
fn a_frame_is_in_the_colours_the_view_sets() {
    let mut mirror = mirror_of(b"a");

    mirror.set_colors(RED, BLUE).unwrap();
    let frame = mirror.frame().unwrap();

    assert_eq!((frame.foreground, frame.background), (RED, BLUE));
    assert_eq!(frame.rows[0].glyphs[0].color, RED);
}

#[test]
fn scrolling_back_shows_the_history_until_it_goes_back_to_the_screen() {
    let lines: Vec<String> = (0..10).map(|line| format!("line {line}")).collect();
    let mut mirror = mirror_of(lines.join("\r\n").as_bytes());
    let text = |frame: &Frame| -> String {
        frame.rows[0].glyphs.iter().map(|g| if let GlyphText::Char(c) = g.text { c } else { '?' }).collect()
    };

    assert!(mirror.wheel(-3).unwrap().is_empty(), "the shell's screen sends nothing for the wheel");
    let back = mirror.frame().unwrap();
    mirror.scroll_to_screen();
    let screen = mirror.frame().unwrap();

    assert_eq!(text(&back), "line3");
    assert_eq!(text(&screen), "line6");
}
