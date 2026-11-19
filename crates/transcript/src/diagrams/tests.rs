use super::{Palette, svg};

fn palette() -> Palette {
    Palette::from(&look::Colors::dark())
}

fn drawn(text: &str) -> Option<String> {
    svg(text, &palette()).map(|svg| String::from_utf8(svg).expect("SVG is text"))
}

#[test]
fn a_diagram_is_drawn_as_svg_in_the_apps_colours_and_its_font() {
    let drawn = drawn("flowchart LR\n  A[Ask] --> B{Decide}\n  B -->|Yes| C[Build]").expect("drawn");

    assert!(drawn.trim_start().starts_with("<svg"), "{drawn}");
    for words in ["Ask", "Decide", "Build", "Yes"] {
        assert!(drawn.contains(words), "says {words}");
    }
    assert!(!drawn.contains("foreignObject"), "as plain text, which isn't only a browser's");
    let raised = look::hex(look::Colors::dark().raised);
    assert!(drawn.contains(&raised), "boxes filled with the app's raised colour, {raised}");
    let family = super::measuring::fonts().expect("a font").family.as_str();
    assert!(drawn.contains(family), "words in {family}, which they were measured in");
}

#[test]
fn a_diagram_that_cant_be_drawn_is_none() {
    assert_eq!(drawn("flowchart LR\n  A[Ask --> B"), None, "a mistake");
    assert_eq!(drawn("This is just words."), None, "no diagram");
    assert_eq!(drawn(""), None, "nothing");
    let long = format!("flowchart LR\n{}", "  A --> B\n".repeat(7000));
    assert_eq!(drawn(&long), None, "too long to start");
}

#[test]
fn only_a_block_written_in_mermaid_is_a_diagram() {
    assert!(super::is_diagram(Some("mermaid")));
    assert!(super::is_diagram(Some("Mermaid")), "as GitHub reads it");
    assert!(!super::is_diagram(Some("rust")));
    assert!(!super::is_diagram(None));
}
