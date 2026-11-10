use super::*;

#[test]
fn stepping_wraps_around_both_ends() {
    assert_eq!(wrapped_step(0, -1, 3), 2);
    assert_eq!(wrapped_step(2, 1, 3), 0);
    assert_eq!(wrapped_step(1, 1, 3), 2);
    assert_eq!(wrapped_step(0, 1, 1), 0);
}
