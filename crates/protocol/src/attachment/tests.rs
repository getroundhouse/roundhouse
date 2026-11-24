use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

use super::*;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
const WEBM: &[u8] = crate::fixtures::WEBM_START;

proptest! {
    #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

    #[test]
    fn a_kept_name_is_never_empty_and_never_a_path(sent in ".{0,300}") {
        let name = file_name(&sent);

        prop_assert!(!name.is_empty());
        prop_assert!(!name.contains('/') && !name.contains('\\'));
        prop_assert!(!name.chars().any(char::is_control));
        prop_assert!(name != "." && name != "..");
        prop_assert!(name.len() <= 200);
        prop_assert!(!name.contains('"'));
    }

    /// Whatever a file starts with, reading which kind it is ends, as one cut short or made up does.
    #[test]
    fn any_bytes_are_some_kind(start in proptest::collection::vec(any::<u8>(), 0..64), cut in 0..WEBM.len()) {
        MediaType::sniff(&start);
        MediaType::sniff(&[WEBM, &start].concat());
        MediaType::sniff(&WEBM[..cut]);
    }
}

#[test]
fn a_file_is_known_by_what_it_starts_with() {
    assert_eq!(MediaType::sniff(PNG), MediaType::Png);
    assert_eq!(MediaType::sniff(&[0xff, 0xd8, 0xff, 0xe0]), MediaType::Jpeg);
    assert_eq!(MediaType::sniff(b"GIF89a..."), MediaType::Gif);
    assert_eq!(MediaType::sniff(b"RIFF\x10\0\0\0WEBPVP8 "), MediaType::Webp);
    assert_eq!(MediaType::sniff("error: héllo\n".as_bytes()), MediaType::Text);
    assert_eq!(MediaType::sniff(b"text with a \0 in it"), MediaType::Other, "NUL means it isn't text");
    assert_eq!(MediaType::sniff(&[0xc3, 0x28]), MediaType::Other, "not UTF-8");
    assert_eq!(MediaType::sniff(b"RIFF\x10\0\0\0WAVEfmt "), MediaType::Other, "a RIFF that isn't WebP");
}

/// A video is known by its container: WebM by the doc type its header names, and MP4 or QuickTime by the brand its
/// `ftyp` box starts with. The same containers hold images and other kinds of video, which aren't these.
#[test]
fn a_video_is_known_by_its_container() {
    let matroska = [&WEBM[..23], b"\x88matroska"].concat();
    let ftyp = |brand: &[u8; 4]| [b"\0\0\0\x18ftyp".as_slice(), brand, b"\0\0\x02\0"].concat();

    assert_eq!(MediaType::sniff(WEBM), MediaType::Webm);
    assert_eq!(MediaType::sniff(&ftyp(b"isom")), MediaType::Mp4);
    assert_eq!(MediaType::sniff(&ftyp(b"mp42")), MediaType::Mp4);
    assert_eq!(MediaType::sniff(&ftyp(b"qt  ")), MediaType::Mov);
    assert_eq!(MediaType::sniff(&matroska), MediaType::Other, "Matroska that isn't WebM");
    assert_eq!(MediaType::sniff(&ftyp(b"heic")), MediaType::Other, "a HEIF image");
    assert_eq!(MediaType::sniff(&ftyp(b"avif")), MediaType::Other, "an AVIF image");
    assert_eq!(MediaType::sniff(&WEBM[..12]), MediaType::Other, "cut short before its doc type");
    assert!(MediaType::Webm.is_shown_in_transcript() && MediaType::Png.is_shown_in_transcript());
    assert!(!MediaType::Mp4.is_shown_in_transcript() && MediaType::Mp4.is_video() && !MediaType::Png.is_video());
}

/// A drawing is text whose first element is `<svg`, whatever comes before it that isn't an element, so text that only
/// mentions one, or an element that only starts the same, is text.
#[test]
fn a_drawing_is_known_by_its_first_element() {
    let drawings: [&[u8]; 5] = [
        b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>",
        b"<svg>",
        b"\xef\xbb\xbf  \n<?xml version=\"1.0\"?>\n<!-- made by hand -->\n<!DOCTYPE svg>\n<svg\n width=\"10\"/>",
        b"<!-- a <svg> in a comment, then --><svg/>",
        b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><svg viewBox=\"0 0 1 1\"></svg>",
    ];
    for drawing in drawings {
        assert_eq!(MediaType::sniff(drawing), MediaType::Svg, "{}", String::from_utf8_lossy(drawing));
    }
    let texts: [&[u8]; 4] =
        [b"draw it as <svg> later", b"<svgfoo/>", b"<html><svg/></html>", b"<!-- never closed <svg/>"];
    for text in texts {
        assert_eq!(MediaType::sniff(text), MediaType::Text, "{}", String::from_utf8_lossy(text));
    }
}

/// A drawing is an image the transcript draws, which the hub keeps, but it's text to a model and to a file's tab.
#[test]
fn a_drawing_is_an_image_that_goes_to_a_model_as_text() {
    assert!(MediaType::Svg.is_image() && MediaType::Png.is_image());
    assert!(!MediaType::Svg.is_bitmap() && MediaType::Png.is_bitmap());
    assert!(MediaType::Svg.is_text() && MediaType::Text.is_text() && !MediaType::Png.is_text());
}

#[test]
fn a_session_takes_any_file_as_what_it_really_is() {
    assert_eq!(MediaType::Png.check(PNG), Ok(()));
    assert_eq!(MediaType::Text.check(b"cargo test\n"), Ok(()));
    assert_eq!(MediaType::Other.check(&[0, 1, 2]), Ok(()));
    assert_eq!(MediaType::Other.check(PNG), Ok(()), "anything can go as a file the agent opens");
    assert_eq!(MediaType::Svg.check(b"<svg/>"), Ok(()));
    assert_eq!(
        MediaType::Text.check(b"<svg/>"),
        Err(AttachmentProblem::NotWhatItSays { said: MediaType::Text, found: MediaType::Svg }),
        "a drawing is said to be one"
    );
    assert_eq!(
        MediaType::Svg.check(b"plain words"),
        Err(AttachmentProblem::NotWhatItSays { said: MediaType::Svg, found: MediaType::Text })
    );

    assert_eq!(
        MediaType::Png.check(&[0xff, 0xd8, 0xff]),
        Err(AttachmentProblem::NotWhatItSays { said: MediaType::Png, found: MediaType::Jpeg })
    );
    assert_eq!(
        MediaType::Text.check(&[0, 1, 2]),
        Err(AttachmentProblem::NotWhatItSays { said: MediaType::Text, found: MediaType::Other })
    );
    assert_eq!(MediaType::Text.check(b""), Err(AttachmentProblem::Empty));
    assert_eq!(MediaType::Unknown.check(PNG), Err(AttachmentProblem::Unsupported));
    let too_long = vec![0; usize::try_from(MAX_FILE_BYTES).unwrap() + 1];
    assert_eq!(MediaType::Other.check(&too_long), Err(AttachmentProblem::TooLarge { limit_bytes: MAX_FILE_BYTES }));
}

#[test]
fn what_a_person_is_told_about_a_refused_file() {
    assert_eq!(AttachmentProblem::TooLarge { limit_bytes: MAX_FILE_BYTES }.to_string(), "a file can be at most 100 MB");
    let renamed = AttachmentProblem::NotWhatItSays { said: MediaType::Png, found: MediaType::Jpeg };
    assert_eq!(renamed.to_string(), "the file says it's image/png but it's image/jpeg");
}

#[test]
fn a_size_reads_the_way_a_person_would_say_it() {
    assert_eq!(describe_size(900), "900 bytes");
    assert_eq!(describe_size(1024), "1 KB");
    assert_eq!(describe_size(1025), "2 KB");
    assert_eq!(describe_size(32 * 1024 * 1024), "32 MB");
    assert_eq!(describe_size(1536 * 1024), "1.5 MB");
}

#[test]
fn a_media_type_travels_as_its_mime_type_and_a_new_one_reads_as_unknown() {
    assert_eq!(crate::frame::encode(&MediaType::Jpeg).unwrap(), b"\"image/jpeg\"\n");
    assert_eq!(crate::frame::encode(&MediaType::Other).unwrap(), b"\"application/octet-stream\"\n");
    assert_eq!(crate::frame::encode(&MediaType::Svg).unwrap(), b"\"image/svg+xml\"\n");
    assert_eq!(crate::frame::encode(&MediaType::Webm).unwrap(), b"\"video/webm\"\n");
    assert_eq!(crate::frame::encode(&MediaType::Mov).unwrap(), b"\"video/quicktime\"\n");
    assert_eq!(crate::frame::decode::<MediaType>(b"\"application/pdf\"").unwrap(), MediaType::Unknown);
}

#[test]
fn a_kept_name_is_the_files_own() {
    assert_eq!(file_name("/Users/someone/Desktop/Screenshot 1.png"), "Screenshot 1.png");
    assert_eq!(file_name("C:\\logs\\build.log"), "build.log");
    assert_eq!(file_name("  \n"), "attachment");
    assert_eq!(file_name("/tmp/.."), "attachment", "a name the worker writes by can't climb out");
    assert_eq!(file_name("."), "attachment");
    assert_eq!(file_name(".env"), ".env");
    assert_eq!(file_name("the \"real\" one.txt"), "the 'real' one.txt");
}
