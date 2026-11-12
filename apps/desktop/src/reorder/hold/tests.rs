use std::time::{Duration, Instant};

use protocol::fixtures::workspace;
use protocol::model::WorkspaceId;

use super::{HOLDS_FOR, Hold, keep_held};

#[test]
fn a_held_order_keeps_each_where_it_was_and_puts_one_that_came_since_after_them_in_the_order_they_were() {
    let workspaces = ["alder", "birch", "cedar", "elm", "fir"].map(workspace);
    let held = ["cedar".into(), "alder".into(), "birch".into()];
    let mut listed = vec![4, 3, 1, 0, 2];

    keep_held(&mut listed, &workspaces, &held);

    assert_eq!(listed, [2, 0, 1, 4, 3]);
}

#[test]
fn the_order_holds_only_while_the_list_shows_what_it_did_so_choosing_another_takes_at_once() {
    let now = Instant::now();
    let order: Vec<WorkspaceId> = vec!["birch".into(), "alder".into()];
    let mut hold = Hold { pointing: true, moved_at: Some(now), ..Hold::default() };
    assert_eq!(hold.held(&"newest", now), None, "nothing has been shown yet");

    hold.showed("newest", order.clone());

    assert_eq!(hold.held(&"newest", now), Some(order.as_slice()));
    assert_eq!(hold.held(&"by name", now), None);
    let rested = now + HOLDS_FOR + Duration::from_millis(1);
    assert_eq!(hold.held(&"newest", rested), None, "nor once the pointer has rested");
}
