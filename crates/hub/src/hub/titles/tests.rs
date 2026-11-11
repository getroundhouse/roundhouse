use protocol::attachment::MediaType;

use super::*;

fn lines(texts: &[&str]) -> Vec<PromptLine> {
    texts.iter().zip(1..).map(|(text, seq)| PromptLine::new(seq, text, 0)).collect()
}

#[test]
fn a_model_is_asked_about_every_prompt_in_order_under_what_to_make_of_them() {
    let asked = work_so_far(&lines(&["Fix the login", "", "Now add\n a test"])).unwrap();

    let expected = format!("{WORK_SO_FAR}\n\n1. Fix the login\n2. Now add a test");
    assert_eq!(asked, expected, "a prompt that only carried files says nothing, so it's passed over");
}

#[test]
fn nothing_typed_is_nothing_to_ask_about() {
    assert_eq!(work_so_far(&[]), None);
    assert_eq!(work_so_far(&lines(&["", " \n "])), None);
}

#[test]
fn too_many_prompts_keep_the_first_and_the_newest_that_fit() {
    let long = "word ".repeat(PromptLine::MOST_CHARS);
    let long = PromptLine::new(1, &long, 0).text().to_string();
    let most = MOST_PROMPT_CHARS / long.chars().count();
    let mut texts = vec!["Fix the login"];
    texts.extend(std::iter::repeat_n(long.as_str(), most + 2));
    texts.push("Now add a test");

    let asked = work_so_far(&lines(&texts)).unwrap();

    let listed: Vec<&str> = asked.strip_prefix(WORK_SO_FAR).unwrap().trim_start().lines().collect();
    assert_eq!(listed[..2], ["1. Fix the login", "…"], "the gap is said");
    let newest = format!("{}. Now add a test", texts.len());
    assert_eq!(listed.last(), Some(&newest.as_str()), "numbered as it was sent");
    assert_eq!(listed.len(), 2 + most, "the first, the gap, and as many of the newest as fit");
    let sent: usize = listed.iter().map(|line| line.chars().count()).sum();
    assert!(sent <= MOST_PROMPT_CHARS + 2 * listed.len(), "{sent}");
}

fn image(id: &str, size_bytes: u64) -> (SessionId, Attachment) {
    let attachment = Attachment { id: id.into(), name: format!("{id}.png"), media_type: MediaType::Png, size_bytes };
    (SessionId::from("s"), attachment)
}

fn ids(chosen: &[(SessionId, Attachment)]) -> Vec<&str> {
    chosen.iter().map(|(_, attachment)| attachment.id.as_str()).collect()
}

#[test]
fn a_title_is_asked_from_the_oldest_images_that_fit() {
    let log = (SessionId::from("s"), Attachment { media_type: MediaType::Text, ..image("log", 1).1 });
    let carried = [log, image("first", MOST_IMAGE_BYTES - 10), image("too big", 11), image("fits", 10), image("x", 1)];

    let chosen = chosen_images(carried);
    let several = chosen_images((1..=MOST_IMAGES + 2).map(|n| image(&n.to_string(), 1)));

    assert_eq!(ids(&chosen), ["first", "fits"], "only images, passing over one too big for the bytes left");
    let oldest: Vec<String> = (1..=MOST_IMAGES).map(|n| n.to_string()).collect();
    assert_eq!(ids(&several), oldest, "the oldest, as many as are allowed");
}
