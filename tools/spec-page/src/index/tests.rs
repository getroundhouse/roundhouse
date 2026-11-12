use std::path::Path;

use super::{Confirmation, area, listed, read};

#[test]
fn a_bold_paragraph_above_an_image_is_its_caption_and_says_whether_its_confirmed() {
    let text = "# Chats\n\n**A finished turn.** *Unconfirmed.*\n\n![A turn](screens/turn.png)\n\n\
                **The composer.**\n\n![The composer](screens/composer.png)\n";

    let area = area("chats.md", "Chats", text).unwrap();

    let screens: Vec<_> = area.screens().collect();
    assert_eq!(screens.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["turn", "composer"]);
    assert_eq!(screens[0].confirmation, Confirmation::Unconfirmed);
    assert_eq!(screens[1].confirmation, Confirmation::Confirmed);
    assert!(screens[0].caption.as_deref().unwrap().contains("A finished turn."));
    assert_eq!((screens[0].image.as_str(), screens[0].alt.as_str()), ("screens/turn.png", "A turn"));
    let blocks = &area.sections[0].blocks;
    assert!(blocks.iter().all(|block| block.html.is_empty()), "a caption isn't also a rule");
}

#[test]
fn a_run_of_images_shares_the_caption_above_it() {
    let text = "# Sidebar\n\n**Replies nobody has read.** *Unconfirmed.*\n\n![In the sidebar](screens/unread.png)\n\n\
                ![In the tab](screens/unread-tab.png)\n";

    let area = area("sidebar.md", "The sidebar", text).unwrap();

    let blocks = &area.sections[0].blocks;
    assert_eq!(blocks.len(), 1, "a run is one row");
    let screens = &blocks[0].screens;
    assert_eq!(screens.len(), 2);
    assert!(screens.iter().all(|screen| screen.confirmation == Confirmation::Unconfirmed));
    assert_eq!(screens[0].caption, screens[1].caption);
}

#[test]
fn an_image_below_anything_but_a_bold_paragraph_has_no_caption() {
    let text = "# Updates\n\n`Restart now` is a quiet button.\n\n![Finishing](screens/finishing.png)\n\n\
                - **A rule in a list.**\n\n![A dialog](screens/dialog.png)\n";

    let area = area("updates.md", "Updates", text).unwrap();

    assert!(area.screens().all(|screen| screen.confirmation == Confirmation::NoCaption));
    assert!(area.screens().all(|screen| screen.caption.is_none()));
    let words: String = area.sections[0].blocks.iter().map(|block| block.html.as_str()).collect();
    assert!(words.contains("is a quiet button") && words.contains("A rule in a list"));
}

#[test]
fn a_screen_under_a_rule_in_its_list_item_shows_that_rule() {
    let text = "# Workspaces\n\n- **The dialog asks what to work on,** and starts on it. *Unconfirmed.*\n\n\
                \x20 **The dialog, open.** *Unconfirmed.*\n\n\
                \x20 ![The dialog](screens/dialog.png)\n\
                - **Return presses Create.**\n";

    let area = area("workspaces.md", "Workspaces", text).unwrap();

    let blocks = &area.sections[0].blocks;
    assert_eq!(blocks.len(), 2, "each rule is a row");
    assert_eq!(blocks[0].lead.as_deref(), Some("<strong>The dialog asks what to work on,</strong>"));
    assert_eq!(blocks[0].screens.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["dialog"]);
    assert!(blocks[0].screens[0].caption.as_deref().unwrap().contains("The dialog, open."));
    assert!(blocks[0].html.contains("and starts on it") && !blocks[0].html.contains("The dialog, open"));
    assert_eq!(blocks[0].unconfirmed, 1, "its screen's caption is the screen's");
    assert!(blocks[1].screens.is_empty());
}

#[test]
fn a_screen_right_under_a_rule_is_captioned_by_it() {
    let text = "# Titles\n\n- **A title is one line.** *Unconfirmed.*\n\n  ![A long title](screens/long.png)\n";

    let area = area("workspaces.md", "Workspaces", text).unwrap();

    let block = &area.sections[0].blocks[0];
    assert!(block.html.contains("A title is one line."), "the rule keeps its words");
    assert_eq!(block.screens[0].confirmation, Confirmation::Unconfirmed);
}

#[test]
fn a_rule_reads_as_it_would_at_the_left_margin() {
    let text = "# Files\n\n- **Press `Cmd-P`,** then:\n  - type a name\n  - press Return\n\n\
                \x20 ```\n  src/\n    main.rs\n  ```\n";

    let area = area("files.md", "Files", text).unwrap();

    let block = &area.sections[0].blocks[0];
    assert_eq!(block.lead.as_deref(), Some("<strong>Press <code>Cmd-P</code>,</strong>"));
    assert!(block.html.contains("<li>type a name</li>\n<li>press Return</li>"), "{}", block.html);
    assert!(block.html.contains("<code>src/\n  main.rs\n</code>"), "{}", block.html);
}

#[test]
fn headings_split_an_area_into_sections_with_their_own_rules_and_screens() {
    let text = "# Workspaces\n\nWhat a workspace is.\n\n## Creating\n\n- **A rule.** *Unconfirmed.*\n\
                - **Another.** *Not built.*\n- **A third, mostly confirmed.** *Unconfirmed:* the copper.\n\n**The dialog.**\n\n![The dialog](screens/new.png)\n\n\
                ## Titles\n\n- **A title.** *Unconfirmed.*\n";

    let area = area("workspaces.md", "Workspaces", text).unwrap();

    assert_eq!(area.title, "Workspaces");
    let titles: Vec<_> = area.sections.iter().map(|section| section.title.as_deref()).collect();
    assert_eq!(titles, [None, Some("Creating"), Some("Titles")]);
    let creating = &area.sections[1].blocks;
    let marks: Vec<_> = creating.iter().map(|block| (block.unconfirmed, block.not_built)).collect();
    assert_eq!(marks, [(1, false), (0, true), (1, false), (0, false)]);
    assert_eq!(creating[3].screens.len(), 1);
    assert!(creating.iter().all(|block| !block.html.contains("The dialog")), "a caption isn't a rule");
    assert_eq!(area.sections[2].blocks.len(), 1);
}

#[test]
fn the_readme_lists_the_areas_in_its_order() {
    let readme = "# Product spec\n\n| File | Covers |\n|---|---|\n| `workspaces.md` | Workspaces |\n\
                  | `chats.md` | Chats, and their titles |\n\nA paragraph naming `look.md`.\n";

    let areas = listed(readme);

    assert_eq!(
        areas,
        [
            ("workspaces.md".to_string(), "Workspaces".to_string()),
            ("chats.md".into(), "Chats, and their titles".into())
        ]
    );
}

#[test]
fn the_spec_reads_with_every_area_it_lists() {
    let spec = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/spec");
    let readme = std::fs::read_to_string(spec.join("README.md")).unwrap();

    let index = read(&spec).unwrap();

    assert_eq!(index.areas.len(), listed(&readme).len());
    assert!(index.areas.iter().all(|area| !area.title.is_empty() && !area.sections.is_empty()));
    let images = index.areas.iter().flat_map(super::Area::screens);
    let missing: Vec<_> = images.filter(|screen| screen.size.is_none()).map(|screen| &screen.image).collect();
    assert!(missing.is_empty(), "images the spec shows that aren't PNGs where it says: {missing:?}");
}

#[test]
fn a_file_the_parser_panics_on_is_refused_as_one_it_cant_read() {
    // A text the parser panics on, as the comment on it in the workspace's manifest says.
    let text = "# Chats\n\na\n===\n---\nb\n===\n";

    let error = area("chats.md", "Chats", text).err().expect("refused");

    assert_eq!(error.to_string(), "chats.md: the parser panicked");
}
