use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

use super::*;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

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

#[test]
fn a_session_takes_any_file_as_what_it_really_is() {
    assert_eq!(MediaType::Png.check(PNG), Ok(()));
    assert_eq!(MediaType::Text.check(b"cargo test\n"), Ok(()));
    assert_eq!(MediaType::Other.check(&[0, 1, 2]), Ok(()));
    assert_eq!(MediaType::Other.check(PNG), Ok(()), "anything can go as a file the agent opens");

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
