//! Reads one of the design system's tables, in `docs/spec/look.md` or `docs/spec/components.md`, so a test can hold it
//! to the names the app has.

/// The spec's files that have the tables: the foundations', and the components'.
const SPEC: [&str; 2] = [include_str!("../../../docs/spec/look.md"), include_str!("../../../docs/spec/components.md")];

/// The first `N` cells of every row of the table after the line that starts with `title`, such as `**Every corner, by
/// its name.**`, past its heading and the line under that, each cell out of its backticks. So a row whose name isn't
/// one the app has, or that's written another way, is read as it is, and fails the test that reads it.
pub(crate) fn rows<const N: usize>(title: &str) -> Vec<[String; N]> {
    let lines = SPEC.iter().flat_map(|file| file.lines());
    let after = lines.skip_while(|line| !line.starts_with(title)).skip(1);
    let table = after.skip_while(|line| !line.starts_with('|')).take_while(|line| line.starts_with('|'));
    table
        .skip(2)
        .map(|row| {
            let mut cells = row.trim_start_matches('|').split('|').map(|cell| cell.trim().trim_matches('`'));
            std::array::from_fn(|_| cells.next().unwrap_or_default().to_string())
        })
        .collect()
}
