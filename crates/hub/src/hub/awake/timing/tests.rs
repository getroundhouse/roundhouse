use super::*;

const DELAYS: Delays = Delays { linger: Duration::from_secs(30), retry: Duration::from_secs(5) };

fn secs(seconds: u64) -> Duration {
    Duration::from_secs(seconds)
}

#[test]
fn work_starts_it_and_it_stops_only_once_the_work_has_been_over_for_a_while() {
    let (mut timing, start) = (Timing::default(), Instant::now());
    assert_eq!(timing.step(Hold::LetGoSoon, false, start, DELAYS), Step::Nothing, "nothing is at work yet");
    assert_eq!(timing.next_step_at(), None);

    assert_eq!(timing.step(Hold::Keep, false, start, DELAYS), Step::Start);
    assert_eq!(timing.step(Hold::Keep, true, start, DELAYS), Step::Nothing);

    assert_eq!(timing.step(Hold::LetGoSoon, true, start + secs(10), DELAYS), Step::Nothing);
    assert_eq!(timing.next_step_at(), Some(start + secs(40)));
    assert_eq!(timing.step(Hold::LetGoSoon, true, start + secs(39), DELAYS), Step::Nothing);
    assert_eq!(timing.step(Hold::LetGoSoon, true, start + secs(40), DELAYS), Step::Stop);
    assert_eq!(timing.next_step_at(), None);
}

#[test]
fn work_that_starts_again_puts_off_letting_go_until_it_next_stops() {
    let (mut timing, start) = (Timing::default(), Instant::now());
    timing.step(Hold::Keep, false, start, DELAYS);
    timing.step(Hold::LetGoSoon, true, start, DELAYS);

    assert_eq!(timing.step(Hold::Keep, true, start + secs(20), DELAYS), Step::Nothing);
    assert_eq!(timing.next_step_at(), None);

    // Past when the first stop would have let go, and counted from the second.
    assert_eq!(timing.step(Hold::LetGoSoon, true, start + secs(25), DELAYS), Step::Nothing);
    assert_eq!(timing.step(Hold::LetGoSoon, true, start + secs(31), DELAYS), Step::Nothing);
    assert_eq!(timing.step(Hold::LetGoSoon, true, start + secs(55), DELAYS), Step::Stop);
}

#[test]
fn once_its_not_wanted_it_stops_at_once_whatever_was_waiting() {
    let (mut timing, start) = (Timing::default(), Instant::now());
    timing.step(Hold::Keep, false, start, DELAYS);
    timing.step(Hold::LetGoSoon, true, start, DELAYS);

    assert_eq!(timing.step(Hold::LetGoNow, true, start + secs(1), DELAYS), Step::Stop);
    assert_eq!(timing.next_step_at(), None);
    assert_eq!(timing.step(Hold::LetGoNow, false, start + secs(2), DELAYS), Step::Nothing);
    assert_eq!(timing.step(Hold::Keep, false, start + secs(3), DELAYS), Step::Start, "wanted again, with work");
}

#[test]
fn one_that_failed_is_started_again_after_a_while_if_theres_still_work() {
    let (mut timing, start) = (Timing::default(), Instant::now());
    timing.step(Hold::Keep, false, start, DELAYS);

    timing.failed(start + secs(1), DELAYS);

    assert_eq!(timing.step(Hold::Keep, false, start + secs(1), DELAYS), Step::Nothing);
    assert_eq!(timing.next_step_at(), Some(start + secs(6)));
    assert_eq!(timing.step(Hold::Keep, false, start + secs(5), DELAYS), Step::Nothing, "a change doesn't hurry it");
    assert_eq!(timing.step(Hold::Keep, false, start + secs(6), DELAYS), Step::Start);
    assert_eq!(timing.next_step_at(), None);
}

#[test]
fn one_that_failed_isnt_started_again_once_the_work_is_over() {
    let (mut timing, start) = (Timing::default(), Instant::now());
    timing.step(Hold::Keep, false, start, DELAYS);
    timing.failed(start, DELAYS);

    assert_eq!(timing.step(Hold::LetGoSoon, false, start + secs(1), DELAYS), Step::Nothing);

    assert_eq!(timing.next_step_at(), None);
    assert_eq!(timing.step(Hold::Keep, false, start + secs(2), DELAYS), Step::Start, "new work starts it at once");
}
