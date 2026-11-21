//! What a terminal's rows say, as a worker writes them, and what a reader makes of a form a newer peer writes.

use protocol::terminal::{CellAttributes, CellColor, CellStyle, Span, TerminalRow, Underline};

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

    keeps_its_form(
        &row,
        concat!(
            r#"{"s":[{"t":"ls"},{"c":4,"n":3,"y":0},{"t":"日本","d":true},"#,
            "{\"t\":\"e\u{301}\",\"y\":0,\"g\":true}],",
            r#""y":[{"f":1,"b":"12abff","a":17,"u":"curly","k":200}],"w":true}"#,
        ),
    );
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
