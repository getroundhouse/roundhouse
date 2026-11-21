//! What a terminal's rows and pictures say, as a worker writes them, and what a reader makes of a form a newer peer
//! writes.

use protocol::terminal::{
    CellAttributes, CellColor, CellStyle, CursorShape, Numbering, PaletteColor, RowJson, RowNumber, RowRun,
    ScreenState, Span, TerminalCursor, TerminalPicture, TerminalRow, TerminalSize, Underline, Wheel,
};
use protocol::worker::WorkerFrame;

use crate::keeps_its_form;

#[test]
fn a_row_keeps_its_form() {
    let style = CellStyle {
        foreground: Some(CellColor::Palette(1)),
        background: Some(CellColor::Rgb { r: 0x12, g: 0xab, b: 0xff }),
        attributes: CellAttributes::BOLD.with(CellAttributes::INVERSE, true),
        underline: Some(Underline::Curly),
        underline_color: Some(CellColor::Palette(200)),
    };
    let spans = vec![
        Span { text: "ls".into(), ..Span::default() },
        Span { col: Some(4), blank: 3, style: Some(0), ..Span::default() },
        Span { text: "日本".into(), wide: true, ..Span::default() },
        Span { text: "e\u{301}".into(), cluster: true, style: Some(0), ..Span::default() },
    ];
    let row = TerminalRow { spans, styles: vec![style], wraps: true };

    let form = concat!(
        r#"{"s":[{"t":"ls"},{"c":4,"n":3,"y":0},{"d":true,"t":"日本"},"#,
        "{\"g\":true,\"t\":\"e\u{301}\",\"y\":0}],",
        r#""w":true,"y":[{"a":17,"b":"12abff","f":1,"k":200,"u":"curly"}]}"#,
    );
    keeps_its_form(&row, form);
    // As a hub reads it, through what serde holds of a frame, and writes it again.
    let json = RowJson::new(&row).unwrap();
    let read_back: RowJson = serde_json::from_value(serde_json::to_value(&json).unwrap()).unwrap();
    assert_eq!(serde_json::to_string(&read_back).unwrap(), form);
    assert_eq!(read_back, json);
    keeps_its_form(&TerminalRow::default(), "{}");
}

/// So a row a newer worker writes, with a colour or a line under its text this build doesn't know, still draws, in the
/// terminal's own colour and with a single line.
#[test]
fn a_colour_or_underline_from_a_newer_peer_reads_as_unknown() {
    let read = |style: &str| serde_json::from_str::<CellStyle>(style).unwrap();

    assert_eq!(read(r#"{"f":"oklch(0.7 0.1 30)"}"#).foreground, Some(CellColor::Unknown));
    assert_eq!(read(r#"{"f":256}"#).foreground, Some(CellColor::Unknown));
    assert_eq!(read(r#"{"f":"12abfz"}"#).foreground, Some(CellColor::Unknown));
    assert_eq!(read(r#"{"f":{"p":3}}"#).foreground, Some(CellColor::Unknown));
    assert_eq!(read(r#"{"u":"zigzag"}"#).underline, Some(Underline::Unknown));
    assert!(!read(r#"{"a":32768}"#).attributes.contains(CellAttributes::BOLD));
    assert_eq!(read(r#"{"z":1}"#), CellStyle::default());
}

#[test]
fn a_picture_keeps_its_form() {
    let row = RowJson::new(&TerminalRow {
        spans: vec![Span { text: "hi".into(), ..Span::default() }],
        ..TerminalRow::default()
    })
    .unwrap();
    let state = ScreenState {
        program: true,
        wheel: Wheel::Arrows,
        title: "vim".into(),
        palette: vec![PaletteColor { index: 1, color: CellColor::Rgb { r: 0xff, g: 0, b: 0 } }],
        foreground: Some(CellColor::Rgb { r: 1, g: 2, b: 3 }),
        background: None,
        cursor_color: None,
        reverse: true,
    };
    let picture = TerminalPicture {
        numbering: Numbering::new(3),
        size: TerminalSize { cols: 80, rows: 24 },
        top: RowNumber::new(900),
        first: RowNumber::new(30),
        rows: vec![RowRun {
            from: RowNumber::new(901),
            rows: vec![row, RowJson::new(&TerminalRow::default()).unwrap()],
        }],
        kept_from: RowNumber::new(800),
        whole: true,
        fresh: true,
        cursor: Some(TerminalCursor { col: 2, row: 1, shape: CursorShape::Bar, blinking: true }),
        state: Some(state),
    };

    keeps_its_form(
        &WorkerFrame::TerminalPicture { terminal: "t".into(), picture },
        concat!(
            r#"{"frame":"terminal_picture","terminal":"t","picture":{"numbering":3,"size":{"cols":80,"rows":24},"#,
            r#""top":900,"first":30,"rows":[{"from":901,"rows":[{"s":[{"t":"hi"}]},{}]}],"kept_from":800,"whole":true,"fresh":true,"#,
            r#""cursor":{"col":2,"row":1,"shape":"bar","blinking":true},"state":{"program":true,"wheel":"arrows","#,
            r#""title":"vim","palette":[{"i":1,"c":"ff0000"}],"foreground":"010203","reverse":true}}}"#,
        ),
    );
}

/// A frame of a picture that isn't the last says only where its rows go.
#[test]
fn a_pictures_frame_before_its_last_says_only_its_rows() {
    let picture: TerminalPicture =
        serde_json::from_str(r#"{"numbering":3,"size":{"cols":80,"rows":24},"top":900,"first":30,"kept_from":900}"#)
            .unwrap();

    assert!(!picture.whole && picture.rows.is_empty() && picture.cursor.is_none() && picture.state.is_none());
}

/// So a picture from a newer worker still draws, with a block for a cursor and the wheel scrolling the history.
#[test]
fn a_cursor_shape_or_wheel_from_a_newer_peer_reads_as_unknown() {
    let cursor: TerminalCursor = serde_json::from_str(r#"{"col":0,"row":0,"shape":"beam"}"#).unwrap();
    let state: ScreenState = serde_json::from_str(r#"{"wheel":"zoom","sixel":true}"#).unwrap();

    assert_eq!(cursor.shape, CursorShape::Unknown);
    assert_eq!(state.wheel, Wheel::Unknown);
}
