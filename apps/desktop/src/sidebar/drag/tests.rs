use std::ops::Range;

use gpui_kit::{Pixels, px};
use proptest::prelude::*;
use proptest::sample::Index;
use proptest::test_runner::{Config, RngSeed};
use protocol::model::{Repo, RepoId, Rev};

use super::super::{Line, places};
use super::{groups, merged, ordered, slot, stack};

fn repo(id: &str) -> Repo {
    Repo { id: RepoId::from(id), name: id.into(), path: format!("/{id}"), created_at_ms: 0, rev: Rev::default() }
}

/// Projects as tall as `heights` say, from 32pt down the list, with the space between each.
fn reaches(heights: &[f32]) -> Vec<Range<Pixels>> {
    stack(heights.iter().map(|&height| px(height)), px(32.))
}

#[test]
fn projects_are_in_the_order_chosen_and_then_the_hubs_for_any_it_doesnt_name() {
    let repos = ["a", "b", "c", "d"].map(repo);

    assert_eq!(ordered(&repos, &[]), [0, 1, 2, 3], "the hub's order until one is dragged");
    let order = ["c", "gone", "a"].map(RepoId::from);
    assert_eq!(ordered(&repos, &order), [2, 0, 1, 3], "and one added since goes after those placed");
}

#[test]
fn the_order_a_drag_saves_keeps_the_projects_it_doesnt_list_after_those_it_does() {
    let ids = |ids: &[&str]| ids.iter().copied().map(RepoId::from).collect::<Vec<_>>();

    let saved = merged(ids(&["b", "a"]), &ids(&["a", "elsewhere", "b", "also-elsewhere"]));

    assert_eq!(saved, ids(&["b", "a", "elsewhere", "also-elsewhere"]));
}

#[test]
fn each_project_reaches_from_its_row_to_its_last_lines_bottom() {
    let resting = [Line::Heading, Line::Repo(1), Line::Workspace(0), Line::Repo(0), Line::Repo(2), Line::Workspace(1)];
    let tops = [0., 32., 70., 114., 154., 192., 228.].map(px);

    let groups = groups(&resting, &tops, |repo| RepoId::from(format!("r{repo}").as_str()));

    let named = |id: &str, reach: Range<f32>| (RepoId::from(id), px(reach.start)..px(reach.end));
    assert_eq!(groups, [named("r1", 32.0..106.), named("r0", 114.0..146.), named("r2", 154.0..228.)]);
}

#[test]
fn a_dragged_project_changes_places_with_the_one_beside_it_once_it_covers_half_of_it() {
    // A short project between two tall ones: 32 to 232, 240 to 280, and 288 to 488. Without the short one, the tall
    // ones' middles are at 132 and 340.
    let groups = reaches(&[200., 40., 200.]);

    assert_eq!(slot(&groups, 1, px(240.)), 1, "where it is");
    assert_eq!(slot(&groups, 1, px(339.)), 1, "covering not quite half of the one after it");
    assert_eq!(slot(&groups, 1, px(340.)), 2, "half of it");
    assert_eq!(slot(&groups, 1, px(133.)), 1, "covering not quite half of the one before it");
    assert_eq!(slot(&groups, 1, px(131.)), 0, "more than half of it");
    assert_eq!(slot(&groups, 1, px(0.)), 0, "above them all");
    assert_eq!(slot(&groups, 1, px(9_000.)), 2, "below them all");
}

proptest! {
    #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

    /// Where a held project goes mustn't depend on where it is, or it would go back the moment it got there, and back
    /// and forth on every frame under a pointer holding still.
    #[test]
    fn where_a_held_project_goes_is_the_same_wherever_it_is_among_the_others(
        heights in prop::collection::vec(1_u16..400, 2..7),
        dragged in any::<Index>(),
        top in 0_u16..3_000,
    ) {
        let mut others: Vec<Pixels> = heights.into_iter().map(|height| px(f32::from(height))).collect();
        let it = others.remove(dragged.index(others.len()));
        let goes_from = |at: usize| {
            let mut heights = others.clone();
            heights.insert(at, it);
            slot(&stack(heights, px(32.)), at, px(f32::from(top)))
        };

        let goes: Vec<usize> = (0..=others.len()).map(goes_from).collect();

        prop_assert!(goes.iter().all(|&to| to == goes[0]), "it goes to {goes:?} from each place in turn");
    }
}

/// Two projects of two workspaces each under the heading, in `order`, with their reaches as the sidebar works them out
/// from where it lays out their lines, with the gap below every project but the last.
fn laid(order: [usize; 2]) -> Vec<Range<Pixels>> {
    let mut resting = vec![Line::Heading];
    for repo in order {
        resting.extend([Line::Repo(repo), Line::Workspace(2 * repo), Line::Workspace(2 * repo + 1)]);
    }
    let groups = groups(&resting, &places(&resting), |repo| RepoId::from(format!("r{repo}").as_str()));
    groups.into_iter().map(|(_, reach)| reach).collect()
}

/// Only the last project has no gap below it, so a project's reach, gap and all, depends on the order. Were where a
/// project goes to depend on that, a project held still could go back and forth on every frame.
#[test]
fn where_a_held_project_goes_is_the_same_whichever_order_its_in_though_only_the_last_has_no_gap_below_it() {
    for top in (0..300_u16).map(|y| px(f32::from(y))) {
        let gone_down = slot(&laid([0, 1]), 0, top);
        let gone_already = slot(&laid([1, 0]), 1, top);
        assert_eq!(gone_down, gone_already, "with its row's top {top:?} down the list");
    }
}
