use std::time::Duration;

use super::super::renewing::{Renewing, wait_after};

#[test]
fn a_renewal_that_goes_on_failing_waits_twice_as_long_each_time_up_to_the_most_it_does() {
    let minutes: Vec<_> = (1..=6).map(|times| wait_after(times).as_secs() / 60).collect();

    assert_eq!(minutes, [1, 2, 4, 8, 10, 10]);
    assert_eq!(wait_after(u32::MAX), Duration::from_mins(10));
}

/// On tokio's paused clock, as nothing here asks anyone anything.
#[tokio::test(start_paused = true)]
async fn a_server_has_one_renewal_under_way_at_once_and_a_stop_waits_for_it_and_lets_no_other_begin() {
    let renewing = Renewing::default();
    let under_way = renewing.begin(&"t".into()).unwrap();
    assert!(renewing.begin(&"t".into()).is_none());
    drop(renewing.begin(&"other".into()).unwrap());
    let mut stopping = tokio::spawn({
        let renewing = renewing.clone();
        async move { renewing.stop().await }
    });

    let waited = tokio::time::timeout(Duration::from_secs(60), &mut stopping).await.is_err();
    drop(under_way);

    stopping.await.unwrap();
    assert!(waited);
    assert!(renewing.begin(&"t".into()).is_none());
}
