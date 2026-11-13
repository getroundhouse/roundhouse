//! Reads one of the tables in `docs/spec/look.md`, so a test can hold it to the names the app has.

/// The spec's file that has the tables.
const LOOK: &str = include_str!("../../../docs/spec/look.md");

/// The first `N` cells of every row of the table after the line that starts with `title`, such as `**Every corner, by
/// its name.**`, past its heading and the line under that, each cell out of its backticks. So a row whose name isn't
/// one the app has, or that's written another way, is read as it is, and fails the test that reads it.
pub(crate) fn rows<const N: usize>(title: &str) -> Vec<[String; N]> {
    let after = LOOK.lines().skip_while(|line| !line.starts_with(title)).skip(1);
    let table = after.skip_while(|line| !line.starts_with('|')).take_while(|line| line.starts_with('|'));
    table
        .skip(2)
        .map(|row| {
            let mut cells = row.trim_start_matches('|').split('|').map(|cell| cell.trim().trim_matches('`'));
            std::array::from_fn(|_| cells.next().unwrap_or_default().to_string())
        })
        .collect()
}
