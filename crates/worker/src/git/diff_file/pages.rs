//! One page of a patch, read as git writes it, so a later page keeps none of what's before it, and git is stopped once
//! the page is full.

use super::super::{git_error, git_failed};
use protocol::client::ApiError;
use std::io;
use std::process::Stdio;
use tokio::io::{AsyncBufRead, AsyncBufReadExt as _, BufReader};
use tokio::process::Command;

/// The lines a page holds, and the line the next page starts at, which is `None` for the last.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Page {
    pub(super) patch: String,
    pub(super) next_line: Option<u32>,
}

/// The page of the patch `diff` writes from its line `first_line`, with at most `most` bytes, as [`read_page`] reads
/// it. Unless git is stopped with lines still to write, it has to have exited well, which for a `--no-index` diff,
/// where `compared`, includes saying the two files differ.
pub(super) async fn patch_page(
    mut diff: Command,
    first_line: u32,
    most: usize,
    compared: bool,
) -> Result<Page, ApiError> {
    // git writes little to stderr, a warning at most, so it can wait to be read until git has written the patch.
    let mut git = diff.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| couldnt("run git", &e))?;
    let written = git.stdout.take().ok_or_else(|| git_error("git's diff was started with nowhere to write"))?;
    let page = read_page(&mut BufReader::new(written), first_line, most).await.map_err(|e| couldnt("read", &e))?;
    if page.next_line.is_some() {
        // Dropping it stops it, since it was made to be killed on drop.
        return Ok(page);
    }
    let exited = git.wait_with_output().await.map_err(|e| couldnt("wait for", &e))?;
    if !(exited.status.success() || compared && exited.status.code() == Some(1)) {
        let said = String::from_utf8_lossy(&exited.stderr);
        return Err(git_failed("git's diff of one file", exited.status, &said));
    }
    Ok(page)
}

fn couldnt(what: &str, e: &io::Error) -> ApiError {
    git_error(format!("couldn't {what} git's diff of one file: {e}"))
}

/// The page of what `lines` holds from its line `first_line`, which is lines up to `most` bytes in all once each is
/// made UTF-8, with any byte that isn't replaced, as a summary's paths are. A line longer than that is a page alone,
/// cut to fit with its newline after it.
pub(super) async fn read_page<R: AsyncBufRead + Unpin>(
    lines: &mut R,
    first_line: u32,
    most: usize,
) -> io::Result<Page> {
    for _ in 0..first_line {
        if line(lines, 0).await?.is_none() {
            return Ok(Page { patch: String::new(), next_line: None });
        }
    }
    let mut patch = String::new();
    let mut taken = 0_u32;
    while let Some((read, cut)) = line(lines, most).await? {
        let text = String::from_utf8_lossy(&read);
        if !cut && patch.len() + text.len() <= most {
            patch.push_str(&text);
            taken += 1;
            continue;
        }
        if !patch.is_empty() {
            // This line starts the next page.
            return Ok(Page { patch, next_line: Some(first_line.saturating_add(taken)) });
        }
        patch.push_str(&text[..text.floor_char_boundary(most.saturating_sub(1))]);
        patch.push('\n');
        let more = !lines.fill_buf().await?.is_empty();
        return Ok(Page { patch, next_line: more.then(|| first_line.saturating_add(1)) });
    }
    Ok(Page { patch, next_line: None })
}

/// The next line `lines` holds, with its newline, and whether it was cut: of a line longer than `most` bytes, its
/// first `most`, with the rest read past. `None` once there are no more.
async fn line<R: AsyncBufRead + Unpin>(lines: &mut R, most: usize) -> io::Result<Option<(Vec<u8>, bool)>> {
    let (mut line, mut cut, mut any) = (Vec::new(), false, false);
    loop {
        let held = lines.fill_buf().await?;
        if held.is_empty() {
            return Ok(any.then_some((line, cut)));
        }
        any = true;
        let (part, ended) = match memchr::memchr(b'\n', held) {
            Some(at) => (&held[..=at], true),
            None => (held, false),
        };
        let room = most.saturating_sub(line.len());
        line.extend_from_slice(&part[..part.len().min(room)]);
        cut |= part.len() > room;
        let used = part.len();
        lines.consume(used);
        if ended {
            return Ok(Some((line, cut)));
        }
    }
}
