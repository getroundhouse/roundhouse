//! What an account's limits are once an agent under it has said some, as each rule of `merged` says.

use protocol::model::{LimitWindow, ModelLabel, UsageLimit};

use super::super::limits_heard::merged;

/// A limit over `window`, of `model` if it's a model's own week, `used_percent` used, as heard at `heard_at_ms`.
fn limit(window: LimitWindow, model: Option<&str>, used_percent: u8, heard_at_ms: Option<i64>) -> UsageLimit {
    UsageLimit { window, model: model.map(ModelLabel::new), used_percent, resets_at_ms: Some(9_000), heard_at_ms }
}

/// A limit kept, as its window, its model, how much is used, and when that was heard.
type Kept<'a> = (LimitWindow, Option<&'a str>, u8, Option<i64>);

/// A case: what it's of, what's heard, whether that's every model's week, when it's heard, and what's kept then.
type Case<'a> = (&'a str, Vec<UsageLimit>, bool, i64, Vec<Kept<'a>>);

/// What's kept, as [`Kept`] says.
fn kept(limits: &[UsageLimit]) -> Vec<Kept<'_>> {
    let each = limits.iter().map(|limit| {
        (limit.window, limit.model.as_ref().map(ModelLabel::as_str), limit.used_percent, limit.heard_at_ms)
    });
    each.collect()
}

#[test]
fn each_window_and_each_models_week_is_kept_by_itself_as_it_was_last_heard() {
    use LimitWindow::{FiveHours, ModelSevenDays, SevenDays, Unknown};
    let week = |used, at| limit(SevenDays, None, used, Some(at));
    let held = vec![week(4, 100), limit(ModelSevenDays, Some("Fable"), 30, Some(100))];
    let cases: Vec<Case<'_>> = vec![
        (
            "hearing of one window leaves the other, and the shortest comes first",
            vec![limit(FiveHours, None, 50, None)],
            false,
            200,
            vec![
                (FiveHours, None, 50, Some(200)),
                (SevenDays, None, 4, Some(100)),
                (ModelSevenDays, Some("Fable"), 30, Some(100)),
            ],
        ),
        (
            "a model's week is kept by its model, trimmed, and one that doesn't say whose isn't",
            vec![limit(ModelSevenDays, Some(" Fable "), 60, None), limit(ModelSevenDays, None, 1, None)],
            false,
            200,
            vec![(SevenDays, None, 4, Some(100)), (ModelSevenDays, Some("Fable"), 60, Some(200))],
        ),
        (
            "every model's week forgets one heard earlier that isn't among them",
            vec![limit(ModelSevenDays, Some("Opus"), 5, None)],
            true,
            200,
            vec![(SevenDays, None, 4, Some(100)), (ModelSevenDays, Some("Opus"), 5, Some(200))],
        ),
        (
            "what doesn't say it's every model's week forgets none",
            vec![limit(ModelSevenDays, Some("Opus"), 5, None)],
            false,
            200,
            vec![
                (SevenDays, None, 4, Some(100)),
                (ModelSevenDays, Some("Fable"), 30, Some(100)),
                (ModelSevenDays, Some("Opus"), 5, Some(200)),
            ],
        ),
        (
            "what was heard earlier never replaces what was heard later, and a model's week heard later stays",
            vec![week(9, 0)],
            true,
            50,
            vec![(SevenDays, None, 4, Some(100)), (ModelSevenDays, Some("Fable"), 30, Some(100))],
        ),
        (
            "a window with no name here, or a week that says a model, is kept as its window alone",
            vec![limit(Unknown, None, 9, None), limit(SevenDays, Some("Fable"), 7, None)],
            false,
            200,
            vec![(SevenDays, None, 7, Some(200)), (ModelSevenDays, Some("Fable"), 30, Some(100))],
        ),
    ];

    for (case, heard, every_models_week, heard_at_ms, expected) in cases {
        let merged = merged(Some(&held), &heard, every_models_week, heard_at_ms);
        assert_eq!(kept(&merged), expected, "{case}");
    }
    assert_eq!(kept(&merged(None, &[week(4, 0)], false, 100)), [(SevenDays, None, 4, Some(100))], "with none held");
}
