#![cfg(test)]

use super::super::screens::FOCUS;
use client_core::fake_hub::{HOUR_MS, NOW_MS, World};
use protocol::model::{LineCounts, Observation, Observed, Rev};

/// `world`, with lines added and removed in the sidebar's workspaces, as the hub last read them, but for the screen's
/// own, which is kept current while it shows: a count large enough to be shortened, ones of a single side, a workspace
/// with no changes, one not yet read, and the archived one's, which nothing draws.
pub(in super::super) fn with_lines(mut world: World) -> World {
    let as_of = |added, removed| Observed::AsOf { value: LineCounts { added, removed }, as_of_ms: NOW_MS - HOUR_MS };
    let lines = [
        ("alder", as_of(37_412, 1_250)),
        ("birch", as_of(48, 0)),
        // cedar's haven't been read.
        (FOCUS, Observed::Current { value: LineCounts { added: 120, removed: 45 } }),
        ("elm", as_of(0, 0)),
        ("fir", as_of(0, 8)),
        ("hazel", as_of(5, 2)),
    ];
    for (rev, (name, lines)) in (1..).zip(lines) {
        let workspace_id = world.workspaces.iter().find(|w| w.name == name).unwrap().id.clone();
        world.observations.push(Observation::WorkspaceLines { workspace_id, lines, rev: Rev::new(rev) });
    }
    world
}
