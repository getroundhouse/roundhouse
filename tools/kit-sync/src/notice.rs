//! The line each file we change carries, as Apache-2.0 asks a changed file to say it was changed.

/// The words of the line, which `third_party/README.md` asks each file we change to hold in a comment near its top.
pub const NOTICE: &str = "Changed in Roundhouse's copy of gpui-kit, as third_party/README.md lists.";

/// `text` without the lines that hold [`NOTICE`], or `None` when it has none, since a change offered upstream is no
/// longer ours.
pub(crate) fn without_notices(text: &[u8]) -> Option<Vec<u8>> {
    let noticed = |line: &[u8]| line.windows(NOTICE.len()).any(|words| words == NOTICE.as_bytes());
    if !noticed(text) {
        return None;
    }
    Some(text.split_inclusive(|byte| *byte == b'\n').filter(|line| !noticed(line)).flatten().copied().collect())
}
