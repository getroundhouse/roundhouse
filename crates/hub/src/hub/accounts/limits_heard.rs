//! What an account's limits are once an agent under it has said some of them: each window kept by itself, and each
//! model's own week by its model, as it was last heard, since an agent says only some at a time, and one that lands
//! late, as one a worker sends again to a hub that connects, is no newer than what was heard since.

use protocol::model::{LimitWindow, ModelLabel, UsageLimit};

/// `held`, the account's limits as kept, with `heard` merged in, as heard at `heard_at_ms`, shortest window first, and
/// models' weeks by their models. A limit heard earlier never replaces one heard later. When `heard` is
/// `every_models_week` the account has, as an answer that lists them all is, a model's week heard earlier that isn't
/// among them has gone, such as one whose label changed, and is forgotten. A window this build has no name for isn't
/// kept, nor a model's week that doesn't say whose, since there'd be nothing to tell either from another by.
pub(crate) fn merged(
    held: Option<&[UsageLimit]>,
    heard: &[UsageLimit],
    every_models_week: bool,
    heard_at_ms: i64,
) -> Vec<UsageLimit> {
    let mut kept: Vec<UsageLimit> = held.map_or_default(<[UsageLimit]>::to_vec);
    for limit in heard.iter().filter_map(|limit| as_kept(limit, heard_at_ms)) {
        match kept.iter_mut().find(|held| (held.window, &held.model) == (limit.window, &limit.model)) {
            Some(held) if held.heard_at_ms.is_some_and(|at_ms| at_ms > heard_at_ms) => {}
            Some(held) => *held = limit,
            None => kept.push(limit),
        }
    }
    if every_models_week {
        // Those it names were kept as heard just now, and one another agent heard later stays.
        let heard_since = |limit: &UsageLimit| limit.heard_at_ms.is_some_and(|at_ms| at_ms >= heard_at_ms);
        kept.retain(|limit| limit.window != LimitWindow::ModelSevenDays || heard_since(limit));
    }
    kept.sort_by(|a, b| (a.window, &a.model).cmp(&(b.window, &b.model)));
    kept
}

/// `limit` as it's kept, as heard at `heard_at_ms`: a model's own week by its model, without spaces around it, and any
/// other window by itself alone, whatever model it says. `None` for one that isn't kept, as [`merged`] says.
fn as_kept(limit: &UsageLimit, heard_at_ms: i64) -> Option<UsageLimit> {
    let model = match limit.window {
        LimitWindow::ModelSevenDays => {
            let named = limit.model.as_ref().map(|model| model.as_str().trim()).filter(|model| !model.is_empty())?;
            Some(ModelLabel::new(named))
        }
        LimitWindow::FiveHours | LimitWindow::SevenDays => None,
        LimitWindow::Unknown => return None,
    };
    Some(UsageLimit { model, heard_at_ms: Some(heard_at_ms), ..limit.clone() })
}
