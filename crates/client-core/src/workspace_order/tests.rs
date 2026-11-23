use protocol::model::Workspace;

use super::Sort;

fn workspace(name: &str, created_at_ms: i64) -> Workspace {
    Workspace { created_at_ms, ..protocol::fixtures::workspace(name) }
}

#[test]
fn each_order_sorts_and_keeps_those_that_come_out_the_same_as_they_were() {
    let mut workspaces = [workspace("cedar", 30), workspace("alder", 10), workspace("Birch", 20), workspace("ash", 10)];
    // The hub says the person last did something in it after the others were made, which they never were.
    workspaces[1].user_active_at_ms = Some(99);
    let sorted = |sort: Sort| {
        let mut listed: Vec<usize> = (0..workspaces.len()).collect();
        sort.sort(&mut listed, &workspaces);
        listed.iter().map(|&i| workspaces[i].name.clone()).collect::<Vec<_>>()
    };

    assert_eq!(sorted(Sort::Oldest), ["alder", "ash", "Birch", "cedar"]);
    assert_eq!(sorted(Sort::Newest), ["cedar", "Birch", "alder", "ash"]);
    assert_eq!(sorted(Sort::LastActive), ["alder", "cedar", "Birch", "ash"]);
    assert_eq!(sorted(Sort::Name), ["alder", "ash", "Birch", "cedar"]);
}

#[test]
fn an_order_is_saved_by_its_name_and_one_this_build_doesnt_know_reads_as_last_active() {
    assert_eq!(serde_json::to_string(&Sort::Oldest).unwrap(), r#""oldest""#);
    assert_eq!(serde_json::from_str::<Sort>(r#""oldest""#).unwrap(), Sort::Oldest);
    assert_eq!(serde_json::from_str::<Sort>(r#""by_colour""#).unwrap(), Sort::LastActive);
}
