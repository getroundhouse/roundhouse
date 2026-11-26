//! The design's index, the canvas `Index`: a row for each other canvas, with a copy of its badge, in the order of the
//! design's levels. Each branch that adds a part would add a row to it by hand, and rows added in the same place
//! conflict as they merge, so it's written from the badges, and written again after a merge.

use crate::error::Error;
use crate::files::{CanvasPath, File};
use crate::status::{BADGE, Status};

/// The index's own canvas, which isn't listed in it.
pub const INDEX: &str = "Index";

/// The design's levels, each a folder at the top of the design, in the order `docs/standards/design.md` has them. The
/// board's own parts, such as the status badge, come last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Foundations,
    Components,
    Patterns,
    Screens,
    Board,
}

impl Level {
    const ALL: [Level; 5] = [Level::Foundations, Level::Components, Level::Patterns, Level::Screens, Level::Board];

    fn name(self) -> &'static str {
        match self {
            Level::Foundations => "Foundations",
            Level::Components => "Components",
            Level::Patterns => "Patterns",
            Level::Screens => "Screens",
            Level::Board => "Board",
        }
    }

    /// The level a canvas is in, by the folder at the top of its path.
    pub fn of(path: &CanvasPath) -> Option<Level> {
        let mut parts = path.parts();
        let folder = parts.next()?;
        parts.next()?;
        Level::ALL.into_iter().find(|level| level.name() == folder)
    }
}

/// What the index lists: each canvas but the index, with its status, in order of level and then of path.
fn entries(canvases: &[File<CanvasPath>]) -> Result<Vec<(CanvasPath, Status)>, Error> {
    let others = canvases.iter().filter(|canvas| canvas.path.as_str() != INDEX);
    let mut entries = others
        .map(|canvas| {
            let level = Level::of(&canvas.path).ok_or_else(|| Error::NoLevel(canvas.path.clone()))?;
            let badges: Vec<Status> = canvas.text.lines().filter_map(Status::of_badge).collect();
            match badges[..] {
                [status] => Ok((level, canvas.path.clone(), status)),
                [] => Err(Error::NoBadge(canvas.path.clone())),
                _ => Err(Error::Badges(canvas.path.clone())),
            }
        })
        .collect::<Result<Vec<_>, Error>>()?;
    entries.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
    Ok(entries.into_iter().map(|(_, path, status)| (path, status)).collect())
}

/// The index's canvas, as Brilliant reads it, for `canvases`. It holds no position or size that Brilliant measures,
/// and names its colours and text by their tokens alone, so Brilliant lays it out and draws it from the design system
/// when it's opened. Each element's ID is made from what it is, so the same design writes the same file.
pub fn index(canvases: &[File<CanvasPath>]) -> Result<String, Error> {
    let mut rows = vec![
        "bp:v1".to_string(),
        "canvas:".to_string(),
        "  backgroundEnabled: false".to_string(),
        "---".to_string(),
        format!(
            "{} al(v,g(8:$spacing.sm),pad(24:$spacing.xl)) p(0,0) s(560,hug) f[({},tok(color.surface.pane))] \"{INDEX}\"",
            id("index"),
            id("index fill"),
        ),
        format!(
            "  {} t(\"{INDEX}\") s(hug,hug) f[({},tok(color.text))] $typography.PageTitle \"Heading\"",
            id("heading"),
            id("heading fill"),
        ),
    ];
    for (path, status) in entries(canvases)? {
        if path.as_str().contains(['"', '\\']) {
            return Err(Error::Unwritable(path));
        }
        let shown = path.parts().collect::<Vec<_>>().join(" / ");
        rows.push(format!("  {} al(h,x(sb),y(c)) s(fill,28) f[] \"{path}\"", id(&format!("{path} row"))));
        rows.push(format!(
            "    {} t(\"{shown}\") s(hug,hug) f[({},tok(color.text))] $typography.Body \"Name\"",
            id(&format!("{path} name")),
            id(&format!("{path} name fill")),
        ));
        rows.push(format!("    {} inst(\"{status}\",canvas(Board/Status)) \"{BADGE}\"", id(&format!("{path} status"))));
    }
    rows.push("bp:end".to_string());
    Ok(rows.join("\n") + "\n")
}

/// What an index lists, each canvas with its status, from its rows as Brilliant keeps them, which it may have laid out
/// and written again since.
pub fn listed(index: &str) -> Vec<(CanvasPath, Status)> {
    let rows: Vec<&str> = index.lines().skip_while(|line| *line != "---").skip(1).collect();
    let starts = rows.iter().enumerate().filter(|(_, row)| row.starts_with("  ") && !row.starts_with("   "));
    let entries = starts.filter_map(|(at, row)| {
        let name = row.rsplit('"').nth(1)?;
        let under = rows[at + 1..].iter().take_while(|row| row.starts_with("    "));
        let status = under.copied().find_map(Status::of_badge)?;
        Some((CanvasPath::new(name), status))
    });
    entries.collect()
}

/// An element's ID, 16 hexadecimal digits as Brilliant's are, from what the element is: FNV-1a, which needs no
/// dependency and spreads short names well enough that a design's few hundred don't meet.
fn id(what: &str) -> String {
    let hash = what
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3));
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::files::canvases;

    fn canvas(path: &str, badge: &str) -> File<CanvasPath> {
        File { path: CanvasPath::new(path), text: format!("1 inst(\"{badge}\",canvas(Board/Status)) \"Status\"") }
    }

    #[test]
    fn the_index_lists_each_canvas_by_level_and_then_by_path_and_reads_back_as_written() {
        let design = [
            canvas("Screens/Desktop/Chats/Plan", "Status Ready for review"),
            canvas("Board/Status", "Status Exploring"),
            canvas("Foundations/Type", "Status Ready"),
            canvas("Foundations/Colours", "Status Exploring"),
            canvas(INDEX, "Status Exploring"),
        ];

        let listed = listed(&index(&design).unwrap());

        let expected = [
            ("Foundations/Colours", Status::Exploring),
            ("Foundations/Type", Status::Ready),
            ("Screens/Desktop/Chats/Plan", Status::ReadyForReview),
            ("Board/Status", Status::Exploring),
        ];
        assert_eq!(listed, expected.map(|(path, status)| (CanvasPath::new(path), status)));
    }

    #[test]
    fn a_canvas_outside_the_levels_or_without_a_badge_or_with_two_is_refused() {
        let outside = index(&[canvas("Sketches/Idea", "Status Exploring")]);
        let loose = index(&[canvas("Idea", "Status Exploring")]);
        let unbadged = index(&[File { path: CanvasPath::new("Components/Chip"), text: String::new() }]);
        let mut twice = canvas("Components/Chip", "Status Ready");
        twice.text.push_str("\n2 inst(\"Status Exploring\",canvas(Board/Status)) \"Status\"");

        assert!(matches!(outside, Err(Error::NoLevel(_))), "{outside:?}");
        assert!(matches!(loose, Err(Error::NoLevel(_))), "{loose:?}");
        assert!(matches!(unbadged, Err(Error::NoBadge(_))), "{unbadged:?}");
        assert!(matches!(index(&[twice]), Err(Error::Badges(_))));
    }

    #[test]
    fn a_copy_of_a_badge_in_an_option_comes_before_the_canvass_own_and_is_not_read_as_it() {
        let mut deciding = canvas("Components/Chip", "Status Ready for review");
        deciding.text.insert_str(0, "1 inst(\"Status Ready\",canvas(Board/Status)) \"Option A\"\n");

        assert_eq!(entries(&[deciding]).unwrap(), [(CanvasPath::new("Components/Chip"), Status::ReadyForReview)]);
    }

    #[test]
    fn the_designs_index_lists_each_canvas_as_its_badge_has_it() {
        let design = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../design");
        let canvases = canvases(&design).unwrap();
        let index = canvases.iter().find(|canvas| canvas.path.as_str() == INDEX).expect("the design's index");

        assert_eq!(
            listed(&index.text),
            entries(&canvases).unwrap(),
            "design/Index.bl isn't as the badges have it: run `cargo run -p design`"
        );
    }
}
