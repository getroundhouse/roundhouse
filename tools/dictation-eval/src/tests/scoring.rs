//! A case's score, what makes a case invalid, and how scores are compared with a baseline.

use dictation::Hints;

use crate::{Case, FieldKind, Invalid, Score, Scores, Term};

fn case(toml: &str) -> Case {
    toml::from_str(toml).unwrap()
}

fn term(written: &str, also: &[&str]) -> Term {
    Term { written: written.into(), also: also.iter().map(|spelling| (*spelling).into()).collect() }
}

/// A composer in a workspace whose chat names `TunnelPool`, with a term the chat never names.
const COMPOSER: &str = r#"
field = "composer"
said = "Keep the TunnelPool and the sqlite-vec index"

[[terms]]
written = "TunnelPool"
also = ["tunnel pool"]

[[terms]]
written = "sqlite-vec"

[workspace]
name = "glenrothes"
branch = "glenrothes/idle-links"
repo = "signalbox"

[[chat]]
agent = "The `TunnelPool` drops idle links at once."

[[chat]]
call = { kind = "read", path = "crates/relay/src/tunnel.rs" }
"#;

#[test]
fn a_term_is_hinted_when_a_hint_favours_any_of_its_spellings_and_the_rest_are_unused() {
    let hints = Hints::new(["IDLE_TIMEOUT", "Ardmore", "relay"]);
    let terms = [term("IDLE_TIMEOUT", &["idle timeout"]), term("TunnelPool", &[]), term("Ardmore", &[])];

    let score = Score::of(&terms, &hints);

    assert_eq!(hints.words(), ["idle timeout", "Ardmore", "relay"], "a snake_case hint is kept only as it's said");
    assert_eq!(score.hinted.iter().collect::<Vec<_>>(), ["Ardmore", "IDLE_TIMEOUT"], "by its also, as said");
    assert_eq!(score.missed.iter().collect::<Vec<_>>(), ["TunnelPool"]);
    assert_eq!((score.hints, score.unused), (3, 1), "only relay favours none of them");
}

/// The one example of a case scored through the field it's dictated into.
#[test]
fn a_case_is_scored_on_the_hints_its_field_gives() {
    let score = case(COMPOSER).score().unwrap();

    assert_eq!(score.hinted.iter().collect::<Vec<_>>(), ["TunnelPool"], "from the chat");
    assert_eq!(score.missed.iter().collect::<Vec<_>>(), ["sqlite-vec"], "named nowhere");
}

#[test]
fn a_case_that_lacks_what_its_field_needs_or_names_what_isn_t_there_is_refused() {
    let answer = case("field = \"answer\"\nsaid = \"Keep them\"");
    let not_said = case(&COMPOSER.replace("and the sqlite-vec index", "and the index"));
    let misspelt = case(&COMPOSER.replace("kind = \"read\"", "kind = \"reed\""));

    assert_eq!(answer.score(), Err(Invalid::Needs(FieldKind::Answer, "question")));
    assert_eq!(not_said.score(), Err(Invalid::NotSaid("sqlite-vec".into())));
    assert_eq!(misspelt.score(), Err(Invalid::UnknownCall(2)));
    assert!(toml::from_str::<Case>("field = \"composer\"\nsaid = \"x\"\nsaying = \"y\"").is_err(), "an unknown key");
}

#[test]
fn a_change_names_each_case_and_each_term_it_gained_lost_took_on_or_left_out() {
    let score = |hinted: &[&str], missed: &[&str], hints| Score {
        hinted: hinted.iter().map(|term| (*term).into()).collect(),
        missed: missed.iter().map(|term| (*term).into()).collect(),
        hints,
        unused: 1,
    };
    let scores = |cases: Vec<(&str, Score)>| {
        let cases: serde_json::Map<_, _> =
            cases.into_iter().map(|(name, score)| (name.into(), serde_json::to_value(score).unwrap())).collect();
        serde_json::from_value::<Scores>(cases.into()).unwrap()
    };
    let then = scores(vec![
        ("changed", score(&["Kept", "Lost", "Dropped"], &["Gained"], 5)),
        ("gone", score(&[], &[], 1)),
        ("same", score(&["Kept"], &[], 2)),
    ]);
    let now = scores(vec![
        ("changed", score(&["Kept", "Gained", "Added"], &["Lost", "Missing"], 7)),
        ("new", score(&[], &[], 1)),
        ("same", score(&["Kept"], &[], 2)),
    ]);

    assert_eq!(
        now.changes_from(&then),
        [
            "gone: no longer a case",
            "changed: a new term, \"Added\", hinted, no longer has the term \"Dropped\", now hints \"Gained\", \
             no longer hints \"Lost\", a new term, \"Missing\", missed, 7 hint words, 1 unused, from 5 and 1",
            "new: a new case",
        ]
    );
    assert!(now.changes_from(&now).is_empty());
}
