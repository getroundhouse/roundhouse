use protocol::attachment::MediaType;
use protocol::event::{ContentBlock, EventBody};

use super::addresses::{self, path};
use super::keep;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\nrest";

fn said(texts: &[&str]) -> EventBody {
    let blocks =
        texts.iter().map(|text| ContentBlock::Text { text: (*text).to_string(), images: Vec::new() }).collect();
    EventBody::Assistant { message_id: None, parent_tool_use_id: None, model: None, blocks, first_index: None }
}

/// What each text block in `body` names, by address.
fn named(body: &EventBody) -> Vec<Vec<&str>> {
    let EventBody::Assistant { blocks, .. } = body else { panic!("{body:?}") };
    blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { images, .. } => Some(images.iter().map(|image| image.address.as_str()).collect()),
            ContentBlock::Thinking { .. }
            | ContentBlock::ToolUse { .. }
            | ContentBlock::ToolResult { .. }
            | ContentBlock::Image { .. }
            | ContentBlock::Unknown => None,
        })
        .collect()
}

fn worktree_with(files: &[(&str, &[u8])]) -> tempfile::TempDir {
    let worktree = tempfile::tempdir().unwrap();
    for (name, bytes) in files {
        let file = worktree.path().join(name);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, bytes).unwrap();
    }
    worktree
}

#[test]
fn an_image_a_text_points_to_is_named_in_its_block_and_sent_with_its_bytes() {
    let worktree = worktree_with(&[("docs/shot.png", PNG)]);
    let mut body = said(&["Done.\n\n![The board](docs/shot.png)"]);

    let kept = keep(worktree.path(), &mut body, u64::MAX);

    let [image] = kept.as_slice() else { panic!("{kept:?}") };
    assert_eq!(image.data.as_slice(), PNG);
    assert_eq!((image.attachment.name.as_str(), image.attachment.media_type), ("shot.png", MediaType::Png));
    let EventBody::Assistant { blocks, .. } = &body else { unreachable!() };
    let [ContentBlock::Text { images, .. }] = blocks.as_slice() else { panic!("{blocks:?}") };
    assert_eq!(images[0].attachment, image.attachment, "the block names the attachment sent beside it");
}

#[test]
fn a_text_the_parser_panics_on_points_to_no_image() {
    assert_eq!(addresses::of("![The board](shot.png)\n\na\n===\n---\nb\n==="), Vec::<String>::new());
}

#[test]
fn an_address_is_read_once_for_an_event_however_many_texts_point_to_it() {
    let worktree = worktree_with(&[("shot.png", PNG)]);
    let mut body = said(&["![one](shot.png)", "![again](shot.png) and ![twice](shot.png)"]);

    let kept = keep(worktree.path(), &mut body, u64::MAX);

    assert_eq!(kept.len(), 1);
    assert_eq!(named(&body), [["shot.png"], ["shot.png"]]);
}

#[test]
fn what_isnt_an_image_inside_the_worktree_is_left_out() {
    let outside = worktree_with(&[("secret.png", PNG)]);
    let worktree = worktree_with(&[("notes.txt", b"not an image"), ("shot.png", PNG)]);
    let elsewhere = outside.path().join("secret.png");
    let text = format!(
        "![a](notes.txt) ![b](missing.png) ![c]({}) ![d](https://example.com/x.png) ![e](shot.png)",
        elsewhere.display()
    );
    let mut body = said(&[&text]);

    let kept = keep(worktree.path(), &mut body, u64::MAX);

    assert_eq!(kept.len(), 1);
    assert_eq!(named(&body), [["shot.png"]]);
}

#[test]
fn images_past_what_an_event_can_carry_are_left_out() {
    let worktree = worktree_with(&[("first.png", PNG), ("second.png", PNG)]);
    let mut body = said(&["![a](first.png) ![b](second.png)"]);

    let kept = keep(worktree.path(), &mut body, u64::try_from(PNG.len()).unwrap());

    assert_eq!(kept.len(), 1);
    assert_eq!(named(&body), [["first.png"]]);
}

#[test]
fn an_event_without_text_is_left_as_it_was() {
    let worktree = worktree_with(&[("shot.png", PNG)]);
    let mut body = EventBody::Notice { level: protocol::event::NoticeLevel::Info, message: "![a](shot.png)".into() };
    let before = body.clone();

    let kept = keep(worktree.path(), &mut body, u64::MAX);

    assert!(kept.is_empty());
    assert_eq!(body, before);
}

#[test]
fn images_are_found_as_markdown_draws_them() {
    let of = |text: &str| addresses::of(text);
    assert_eq!(of("![a](one.png) then ![b](two.png) and ![a](one.png)"), ["one.png", "two.png"]);
    assert_eq!(of("![The board][board]\n\n[board]: docs/board.png"), ["docs/board.png"]);
    assert_eq!(of("![a](<a shot.png>)"), ["a shot.png"], "an address in brackets can have spaces");
    assert_eq!(of("`![a](code.png)`\n\n```\n![b](fenced.png)\n```"), Vec::<String>::new(), "code isn't drawn");
    assert_eq!(of("[a link](page.png)"), Vec::<String>::new(), "a link isn't an image");
    assert_eq!(of("It costs $5 ![a](one.png) or $10"), ["one.png"], "dollars aren't math");
}

#[test]
fn an_image_in_the_workspace_is_named_by_its_path_however_it_was_written() {
    assert_eq!(path("docs/shot.png").as_deref(), Some("docs/shot.png"));
    assert_eq!(path("/work/repo/shot.png").as_deref(), Some("/work/repo/shot.png"));
    assert_eq!(path("file:///work/repo/a%20shot.png").as_deref(), Some("/work/repo/a shot.png"));
    assert_eq!(path("100%.png").as_deref(), Some("100%.png"), "a % that escapes nothing stays");
    assert_eq!(path("%FF.png").as_deref(), Some("%FF.png"), "so does one that isn't text once it's decoded");
}

#[test]
fn an_image_on_the_web_or_written_into_the_text_isnt_read_from_the_worktree() {
    assert_eq!(path("https://example.com/shot.png"), None);
    assert_eq!(path("data:image/png;base64,R0lGODlh"), None);
    assert_eq!(path(""), None);
}
