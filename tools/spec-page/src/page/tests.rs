use crate::index::{Index, Size, area};

use super::{draw, write};

fn index(text: &str) -> Index {
    let mut area = area("menus.md", "Menus & <dialogs>", text).unwrap();
    let blocks = area.sections.iter_mut().flat_map(|section| &mut section.blocks);
    for screen in blocks.flat_map(|block| &mut block.screens) {
        screen.size = Some(Size { width: 1200, height: 800 });
    }
    Index { areas: vec![area] }
}

#[test]
fn the_spec_shows_as_html_and_what_the_page_says_about_it_is_escaped() {
    let text = "# Menus & <dialogs>\n\n- **A rule** with `code`.\n\n**The menu.** *Unconfirmed.*\n\n\
                ![A \"menu\"](screens/menu.png)\n";

    let page = draw(&index(text)).into_string();

    assert!(page.contains("<h2>Menus &amp; &lt;dialogs&gt;</h2>"), "{page}");
    assert!(page.contains("<span class=\"lead\"><strong>A rule</strong></span>"), "{page}");
    assert!(page.contains("<p><strong>A rule</strong> with <code>code</code>.</p>"), "{page}");
    assert!(page.contains("alt=\"A &quot;menu&quot;\""), "{page}");
}

#[test]
fn each_screen_shows_its_status_at_its_size_in_the_app_with_its_marks_picked_out() {
    let text = "# Menus\n\n- **A rule.** *Unconfirmed:* the copper.\n\n**The menu.** *Unconfirmed.*\n\n\
                ![The menu](screens/menu.png)\n";

    let page = draw(&index(text)).into_string();

    let rule = "<div class=\"row to-confirm\"><div class=\"words\"><details><summary><span class=\"lead\">\
                <strong>A rule.</strong></span> <em class=\"mark unconfirmed\">Unconfirmed</em> \
                <span class=\"no-picture\">No picture</span></summary>";
    assert!(page.contains(rule), "{page}");

    assert!(page.contains("<figure class=\"screen unconfirmed\" id=\"screen-menu\">"), "{page}");
    assert!(page.contains("src=\"screens/menu.png\" width=\"600\" height=\"400\""), "{page}");
    assert!(page.contains("<em class=\"mark unconfirmed\">Unconfirmed:</em> the copper."), "{page}");
}

#[test]
fn the_page_is_written_with_its_index_and_the_spec_s_screens_as_they_are_now() {
    let spec = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    std::fs::create_dir(spec.path().join("screens")).unwrap();
    std::fs::write(spec.path().join("screens/menu.png"), "the menu").unwrap();
    std::fs::create_dir(out.path().join("screens")).unwrap();
    std::fs::write(out.path().join("screens/dropped.png"), "a screen the spec dropped").unwrap();

    let page = write(&index("# Menus\n"), spec.path(), out.path()).unwrap();

    assert!(std::fs::read_to_string(page).unwrap().contains("<h1>The spec</h1>"));
    assert!(std::fs::read_to_string(out.path().join("index.json")).unwrap().contains("\"menus.md\""));
    assert_eq!(std::fs::read_to_string(out.path().join("screens/menu.png")).unwrap(), "the menu");
    assert!(!out.path().join("screens/dropped.png").exists());
}
