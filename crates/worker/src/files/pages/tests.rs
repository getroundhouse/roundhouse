use std::fmt::Write as _;

use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};
use protocol::client::ErrorCode;
use protocol::model::{CommitId, FileOpened, PageLine};

use super::{LINE_BYTES, Limits, MAX_TEXT_BYTES, PAGE_LINES, Paging, Reads, Wanted, open_at, pages};
use crate::git::blob::Streamed;
use crate::git::tests::{new_repo, run};

/// Each page's index and lines, as text and whether each was cut.
type Texts = Vec<(u32, Vec<(String, bool)>)>;

/// No limit to speak of.
const ANY: Limits = Limits { line: usize::MAX, reply: u64::MAX };

/// What `paging` keeps of `text`, given to it `chunk` bytes at a time until it says to stop: its pages, how many lines
/// it counted, whether it's text, and whether it stopped early.
fn paged(mut paging: Paging, text: &[u8], chunk: usize) -> (Texts, u32, bool, bool) {
    let mut stopped = false;
    for part in text.chunks(chunk) {
        if paging.feed(part).is_break() {
            stopped = true;
            break;
        }
    }
    let read = paging.finish(&Streamed { bytes: 0, stopped });
    let texts = |lines: Vec<PageLine>| lines.into_iter().map(|line| (line.text, line.cut.is_some())).collect();
    let pages = read.pages.into_iter().map(|page| (page.index, texts(page.lines))).collect();
    (pages, read.lines, read.text, stopped)
}

fn whole(lines: &[&str]) -> Vec<(String, bool)> {
    lines.iter().map(|line| ((*line).to_string(), false)).collect()
}

#[test]
fn lines_lose_their_endings_and_a_last_one_without_one_is_counted() {
    let paging = Paging::new(Wanted::All, Reads::ToTheEnd, 2, ANY);

    let (pages, lines, text, _) = paged(paging, b"one\r\ntwo\n\nfour", 3);

    assert_eq!(pages, [(0, whole(&["one", "two"])), (1, whole(&["", "four"]))]);
    assert_eq!((lines, text), (4, true));
    assert_eq!(paged(Paging::new(Wanted::All, Reads::ToTheEnd, 2, ANY), b"", 3).1, 0, "an empty file has no lines");
}

#[test]
fn reading_as_far_as_needed_stops_after_the_last_page_and_has_none_past_the_end() {
    let text = b"1\n2\n3\n4\n5\n6\n7\n";

    let (pages, _, _, stopped) = paged(Paging::new(Wanted::Pages(vec![1, 0]), Reads::AsFarAsNeeded, 2, ANY), text, 1);
    let (past, ..) = paged(Paging::new(Wanted::Pages(vec![3, 9]), Reads::AsFarAsNeeded, 2, ANY), text, 4);

    assert_eq!(pages, [(0, whole(&["1", "2"])), (1, whole(&["3", "4"]))], "in the file's order");
    assert!(stopped, "without reading the rest");
    assert_eq!(past, [(3, whole(&["7"]))], "the last page has what's left, and there's none past it");
}

#[test]
fn a_line_too_long_to_send_is_cut_where_a_character_ends_and_says_how_long_it_is() {
    // A character of three bytes across the cut, after a byte that isn't UTF-8.
    let long = "\u{fffd}ab€ and more\n".replace('\u{fffd}', "").into_bytes();
    let long = [&[0xFF][..], &long].concat();
    let mut paging = Paging::new(Wanted::All, Reads::ToTheEnd, 2, Limits { line: 5, reply: u64::MAX });
    for part in long.chunks(2) {
        let _ = paging.feed(part);
    }

    let line = paging.finish(&Streamed { bytes: 0, stopped: false }).pages.remove(0).lines.remove(0);

    assert_eq!((line.text.as_str(), line.cut), ("\u{fffd}ab", Some(u64::try_from(long.len() - 1).unwrap())));
}

#[test]
fn a_carriage_return_past_the_cut_doesnt_count_as_the_line_s() {
    let paging = Paging::new(Wanted::All, Reads::ToTheEnd, 2, Limits { line: 3, reply: u64::MAX });

    let (pages, ..) = paged(paging, b"abc\r\nabcd\r\n", 1);

    assert_eq!(pages, [(0, vec![("abc".to_string(), false), ("abc".to_string(), true)])]);
}

#[test]
fn pages_are_kept_while_they_fit_and_always_the_first() {
    // Each line counts as its text and what it takes to send.
    let reply = 2 * (3 + LINE_BYTES);
    let paging = |reply| Paging::new(Wanted::Pages(vec![0, 1, 2]), Reads::AsFarAsNeeded, 2, Limits { line: 9, reply });

    let (fits, ..) = paged(paging(reply), b"abc\nabc\nabc\nabc\n", 3);
    let (first, ..) = paged(paging(1), b"abc\nabc\nabc\nabc\n", 3);

    assert_eq!(fits.iter().map(|(index, _)| *index).collect::<Vec<_>>(), [0], "the second doesn't fit beside it");
    assert_eq!(first.iter().map(|(index, _)| *index).collect::<Vec<_>>(), [0], "though it alone doesn't fit");
}

#[test]
fn a_file_with_a_nul_near_its_start_isn_t_text() {
    let mut binary = b"a\n\0".to_vec();
    binary.resize(20_000, b'b');

    let (.., text, stopped) = paged(Paging::new(Wanted::All, Reads::ToTheEnd, 2, ANY), &binary, 1000);
    let (.., short, _) = paged(Paging::new(Wanted::All, Reads::ToTheEnd, 2, ANY), b"a\0", 1000);

    assert!(!text && stopped, "found without reading the rest");
    assert!(!short, "and one shorter than what's looked at");
}

/// `text`'s lines, without their endings, and cut to `line` bytes.
fn lines_of(text: &str, line: usize) -> Vec<(String, bool)> {
    text.split_inclusive('\n')
        .map(|whole| whole.strip_suffix('\n').unwrap_or(whole))
        .map(|whole| whole.strip_suffix('\r').unwrap_or(whole))
        .map(|whole| (whole[..whole.len().min(line)].to_string(), whole.len() > line))
        .collect()
}

/// The pages `wanted` of `lines`, every one if it's `None`, with `size` lines a page, kept while they fit in `reply`.
fn pages_of(lines: &[(String, bool)], wanted: Option<Vec<u32>>, size: u32, reply: u64) -> Texts {
    let every = u32::try_from(lines.len().div_ceil(usize::try_from(size).unwrap())).unwrap();
    let mut wanted = wanted.unwrap_or_else(|| (0..every).collect());
    wanted.sort_unstable();
    wanted.dedup();
    let size = usize::try_from(size).unwrap();
    let mut sent = 0;
    let mut kept = Vec::new();
    for page in wanted.into_iter().filter(|page| usize::try_from(*page).unwrap() * size < lines.len()) {
        let first = usize::try_from(page).unwrap() * size;
        let lines = lines[first..(first + size).min(lines.len())].to_vec();
        let bytes: u64 = lines.iter().map(|(text, _)| u64::try_from(text.len()).unwrap() + LINE_BYTES).sum();
        if !kept.is_empty() && sent + bytes > reply {
            break;
        }
        sent += bytes;
        kept.push((page, lines));
    }
    kept
}

proptest! {
    #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

    /// However the file comes in, each page kept is the file's own lines, cut to the limit for a line, in the order
    /// the file has them while they fit in a reply, and it counts as many lines as a split at each line's end makes.
    #[test]
    fn the_pages_kept_are_the_file_s_own_lines(
        text in "[ab\r\n]{0,60}",
        wanted in proptest::option::of(proptest::collection::vec(0_u32..25, 0..6)),
        (size, line, reply) in (1_u32..5, 1_usize..6, 1_u64..120),
        chunk in 1_usize..9,
        to_the_end: bool,
    ) {
        let expected = pages_of(&lines_of(&text, line), wanted.clone(), size, reply);
        let reads = if to_the_end || wanted.is_none() { Reads::ToTheEnd } else { Reads::AsFarAsNeeded };
        let wanting = wanted.map_or(Wanted::All, Wanted::Pages);

        let paging = Paging::new(wanting, reads, size, Limits { line, reply });
        let (pages, counted, ..) = paged(paging, text.as_bytes(), chunk);

        prop_assert_eq!(pages, expected);
        if reads == Reads::ToTheEnd {
            prop_assert_eq!(counted, u32::try_from(lines_of(&text, line).len()).unwrap());
        }
    }
}

/// Past text's limit, so it couldn't be sent whole.
#[tokio::test]
async fn a_file_too_big_to_send_whole_is_opened_and_read_a_page_at_a_time() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let numbered = (1..=200_000).fold(String::new(), |mut text, n| {
        let _ = writeln!(text, "line {n:>20}");
        text
    });
    assert!(u64::try_from(numbered.len()).unwrap() > MAX_TEXT_BYTES);
    std::fs::write(repo.join("big.log"), &numbered).unwrap();
    std::fs::write(repo.join("long.txt"), format!("{}\n", "a".repeat(100_000))).unwrap();
    std::fs::write(repo.join("data.bin"), b"a\0b\n").unwrap();
    run(&repo, &["add", "."]);
    run(&repo, &["commit", "-qm", "files"]);
    let commit = CommitId::from(run(&repo, &["rev-parse", "HEAD"]));

    let FileOpened::Text(big) = open_at(&repo, &commit, "big.log").await.unwrap() else { panic!("not text") };
    let FileOpened::Text(long) = open_at(&repo, &commit, "long.txt").await.unwrap() else { panic!("not text") };
    let read = pages(&repo, &commit, "big.log", big.page_lines, &[586]).await.unwrap();

    assert_eq!((big.lines, big.page_lines, big.pages.len()), (200_000, PAGE_LINES, 0), "none sent with it");
    let line = &long.pages[0].lines[0];
    assert_eq!((line.text.len(), line.cut), (100_000, None), "a small one comes whole, its long lines too");
    let first = read.pages.first().map(|page| (page.index, page.lines[0].text.clone()));
    assert_eq!(first, Some((586, format!("line {:>20}", 586 * PAGE_LINES + 1))));
    assert_eq!(open_at(&repo, &commit, "data.bin").await.unwrap(), FileOpened::NotText);
}

#[tokio::test]
async fn pages_of_what_the_commit_doesnt_hold_or_of_no_size_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let commit = CommitId::from(run(&repo, &["rev-parse", "HEAD"]));
    let refused = async |path, size| pages(&repo, &commit, path, size, &[0]).await.unwrap_err().code;

    assert_eq!(refused("none.txt", 256).await, ErrorCode::NotFound);
    assert_eq!(refused("a.txt", 0).await, ErrorCode::InvalidRequest);
    assert_eq!(refused("../a.txt", 256).await, ErrorCode::InvalidRequest, "a path out of the worktree");
}
