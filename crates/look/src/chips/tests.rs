use super::{ChipKind, ChipSize, IssueState, PullRequestState, chip};
use crate::spec_table;

#[test]
fn the_specs_table_has_every_kind_of_chip_in_order() {
    let table: Vec<[String; 1]> = spec_table::rows("**Every kind of chip, by its name.**");

    let kinds: Vec<[String; 1]> = ChipKind::ALL.iter().map(|kind| kind.written()).collect();
    assert_eq!(table, kinds, "docs/spec/components.md's table of kinds of chip, against crates/look/src/chips");
}

#[test]
fn the_specs_table_has_every_size_of_chip_in_order() {
    let table: Vec<[String; 4]> = spec_table::rows("**Every size of a chip, by its name.**");

    let sizes: Vec<[String; 4]> = ChipSize::ALL.iter().map(|size| size.written()).collect();
    assert_eq!(table, sizes, "docs/spec/components.md's table of a chip's sizes, against crates/look/src/chips");
}

#[test]
fn the_specs_tables_have_every_state_of_an_issue_and_a_pull_request_in_order() {
    let issues: Vec<[String; 3]> = spec_table::rows("**Every state of an issue's chip, by its name.**");
    let pull_requests: Vec<[String; 3]> = spec_table::rows("**Every state of a pull request's chip, by its name.**");

    let issue_states: Vec<[String; 3]> = IssueState::ALL.iter().map(|state| state.written()).collect();
    let pull_request_states: Vec<[String; 3]> = PullRequestState::ALL.iter().map(|state| state.written()).collect();
    assert_eq!(
        issues, issue_states,
        "docs/spec/components.md's table of an issue's states, against crates/look/src/chips"
    );
    assert_eq!(
        pull_requests, pull_request_states,
        "docs/spec/components.md's table of a pull request's states, against crates/look/src/chips"
    );
}

/// A file or a folder shows the last part of its path, and the rest show what they're named whole.
#[test]
fn a_chip_shows_a_paths_last_part() {
    assert_eq!(chip(ChipKind::File, ChipSize::Regular, "crates/protocol/src/model.rs").name(), "model.rs");
    assert_eq!(chip(ChipKind::Folder, ChipSize::Regular, "crates/protocol/").name(), "protocol");
    assert_eq!(chip(ChipKind::File, ChipSize::Regular, "README.md").name(), "README.md");
    assert_eq!(
        chip(ChipKind::Subagent, ChipSize::Regular, "Find every tooltip / in look").name(),
        "Find every tooltip / in look"
    );
}

/// The lines a chip is about follow its name as an editor writes a place in a file.
#[test]
fn a_chips_lines_are_written_as_a_place_in_a_file() {
    let file = || chip(ChipKind::File, ChipSize::Regular, "model.rs");

    assert_eq!(file().lines(12, Some(40)).lines_written().as_deref(), Some(":12\u{2013}40"));
    assert_eq!(file().lines(12, Some(12)).lines_written().as_deref(), Some(":12"));
    assert_eq!(file().lines(12, None).lines_written().as_deref(), Some(":12\u{2013}"));
    assert_eq!(file().lines_written(), None);
}
