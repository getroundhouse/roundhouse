use super::{Decoder, Encoded, MAX_LIVE_LAG, MOST_A_STEP, Token};
use crate::Hints;
use crate::hints::HintGraph;
use crate::nemotron::parts::Tensor;
use crate::nemotron::stand_in::{self, ScriptedTransducer};
use crate::nemotron::transducer::{BLANK, ENCODED};
use crate::nemotron::windows::Steps;

/// What an encoder made out of a window at `first`, each step's values all the step's own.
fn encoded(steps: &[f32], first: Steps) -> Encoded {
    let values = (0..ENCODED).flat_map(|_| steps.iter().copied()).collect();
    let encoder = Tensor { values, shape: vec![1, ENCODED, steps.len()] };
    Encoded::new(encoder, &stand_in::count(steps.len()), first).unwrap()
}

fn ids(tokens: &[Token]) -> Vec<(usize, usize)> {
    tokens.iter().map(|token| (token.id, token.at.0)).collect()
}

fn empty_hints() -> HintGraph {
    HintGraph::new(&[], &Hints::default(), 0.0)
}

#[test]
fn decoding_a_window_in_two_goes_on_from_the_tokens_so_far_as_decoding_it_whole_does() {
    let parts = stand_in::counting_parts();
    let hints = empty_hints();
    let window = encoded(&[1.0, 1.0, 3.0, 3.0, 4.0, 6.0], Steps(10));

    let whole = Decoder::new().decode_all(&*parts.transducer, &hints, &window, Steps(10)..Steps(16)).unwrap();
    let mut halves = Decoder::new();
    let mut split = halves.decode_live(&*parts.transducer, &hints, &window, Steps(10)..Steps(13)).unwrap();
    split.extend(halves.decode_live(&*parts.transducer, &hints, &window, Steps(13)..Steps(16)).unwrap());
    split.extend(halves.finish_live(&hints));

    assert_eq!(ids(&whole), [(1, 10), (2, 12), (3, 12), (4, 14), (5, 15), (6, 15)]);
    assert_eq!(ids(&split), ids(&whole));
}

#[test]
fn a_step_gives_at_most_ten_tokens_and_the_next_is_decoded() {
    let parts = stand_in::never_done_parts();
    let hints = empty_hints();
    let window = encoded(&[0.0, 0.0], Steps(0));

    let tokens = Decoder::new().decode_all(&*parts.transducer, &hints, &window, Steps(0)..Steps(2)).unwrap();

    assert_eq!(tokens.len(), 2 * MOST_A_STEP);
    assert!(tokens[..MOST_A_STEP].iter().all(|token| token.at == Steps(0)));
    assert!(tokens[MOST_A_STEP..].iter().all(|token| token.at == Steps(1)));
}

#[test]
fn a_step_is_read_down_its_column_and_an_encoder_of_another_shape_is_refused() {
    let steps = 3;
    let values = (0..ENCODED).flat_map(|v| (0..steps).map(move |s| stand_in::real(100 * s + v))).collect();
    let encoder = Tensor { values, shape: vec![1, ENCODED, steps] };
    let window = Encoded::new(encoder, &stand_in::count(steps), Steps(7)).unwrap();

    let step = window.step(Steps(8));
    assert_eq!((step[0], step[5]), (100.0, 105.0));
    assert_eq!(window.made_out(), Steps(10));
    let values = vec![0.0; ENCODED * steps];
    let longer = Encoded::new(Tensor { values, shape: vec![1, ENCODED, steps] }, &stand_in::count(9), Steps(7));
    assert_eq!(longer.unwrap().made_out(), Steps(10), "no more steps than it gave");

    let wrong = Tensor { values: vec![0.0; ENCODED * steps], shape: vec![1, steps, ENCODED] };
    assert!(Encoded::new(wrong, &stand_in::count(steps), Steps(0)).is_err());
}

#[test]
fn beam_search_picks_hinted_homophone_and_rolls_back_partial_hint_matches() {
    let vocab: Vec<String> = [
        "<unk>", "▁T", "ok", "io", "▁w", "▁wh", "ol", "ole", "▁h", "▁hu", "▁H", "▁Hu", "▁c", "▁cr", "at", "ate", "▁",
        "H", "T", "a", "b", "c", "e", "h", "i", "k", "l", "o", "r", "t", "u", "w", "y",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    let id = |piece: &str| vocab.iter().position(|p| p == piece).unwrap();
    let (t_tok, ok, io, y, o) = (id("▁T"), id("ok"), id("io"), id("y"), id("o"));
    let (wh, ole, hu, b) = (id("▁wh"), id("ole"), id("▁hu"), id("b"));
    let hints = HintGraph::new(&vocab, &Hints::new(["Tokio", "hub crate"]), 2.0);
    // Steps 0..2 sound slightly more like "Tokyo" than "Tokio", and Steps 2..4 sound like "whole", with a weak
    // acoustic false start on "▁hu" that fails on step 3.
    let transducer = ScriptedTransducer(move |step, history: &[usize]| match (step, history) {
        (0, []) => vec![(t_tok, 5.0), (BLANK, -5.0)],
        (0, [first]) if *first == t_tok => vec![(ok, 5.0), (BLANK, -5.0)],
        (1, [first, second]) if *first == t_tok && *second == ok => vec![(y, 4.0), (io, 2.5), (BLANK, -5.0)],
        (1, [first, second, third]) if *first == t_tok && *second == ok && *third == y => {
            vec![(o, 5.0), (BLANK, -5.0)]
        }
        (2, path) if path == [t_tok, ok, io] || path == [t_tok, ok, y, o] => {
            vec![(wh, 4.0), (hu, 3.2), (BLANK, -5.0)]
        }
        (3, path) if path.last() == Some(&wh) => vec![(ole, 5.0), (BLANK, -5.0)],
        (3, path) if path.last() == Some(&hu) => vec![(ole, 4.0), (b, -20.0), (BLANK, -5.0)],
        _ => vec![(BLANK, 10.0)],
    });
    let window = encoded(&[0.0, 1.0, 2.0, 3.0], Steps(0));

    let without_hints = Decoder::new().decode_all(&transducer, &empty_hints(), &window, Steps(0)..Steps(4)).unwrap();
    let with_hints = Decoder::new().decode_all(&transducer, &hints, &window, Steps(0)..Steps(4)).unwrap();

    let text_of = |tokens: &[Token]| -> String {
        tokens.iter().map(|t| vocab[t.id].as_str()).collect::<String>().replace('▁', " ").trim().to_string()
    };
    assert_eq!(text_of(&without_hints), "Tokyo whole");
    assert_eq!(text_of(&with_hints), "Tokio whole", "Tokio wins, while '▁hu' rolls back so 'whole' isn't corrupted");

    // Live streaming holds back the undecided suffix until `finish_live` settles the boundary, keeping `pending_from`
    // at the earliest undecided step.
    let mut live = Decoder::new();
    let first_half = live.decode_live(&transducer, &hints, &window, Steps(0)..Steps(2)).unwrap();
    assert_eq!(ids(&first_half), [(t_tok, 0), (ok, 0)], "only the agreed prefix '▁T' + 'ok' is emitted mid-word");
    assert_eq!(live.pending_from(), Some(Steps(1)), "step 1 still differs between 'io' and 'yo'");
    let mut stream = first_half;
    stream.extend(live.decode_live(&transducer, &hints, &window, Steps(2)..Steps(4)).unwrap());
    stream.extend(live.finish_live(&hints));
    assert_eq!(live.pending_from(), None);
    assert_eq!(text_of(&stream), "Tokio whole");
}

#[test]
fn live_lag_limit_drops_early_runner_up_token_when_best_settled_beam_stayed_silent() {
    let vocab: Vec<String> =
        ["<unk>", "▁T", "ok", "io", "▁", "T", "o", "k", "i"].into_iter().map(String::from).collect();
    let hints = HintGraph::new(&vocab, &Hints::new(["Tokio"]), 2.0);
    // Step 0 slightly prefers silence (`BLANK`), while a runner-up beam emits token 5 (`"T"`) within `BEAM_FLOOR`.
    // Every later step is silent.
    let transducer = ScriptedTransducer(|step, history: &[usize]| match (step, history) {
        (0, []) => vec![(5, 1.5), (BLANK, 4.0)],
        _ => vec![(BLANK, 10.0)],
    });
    let total = MAX_LIVE_LAG.0 + 3;
    let step_vals: Vec<f32> = (0..total).map(stand_in::real).collect();
    let window = encoded(&step_vals, Steps(0));

    let mut live = Decoder::new();
    assert!(live.decode_live(&transducer, &hints, &window, Steps(0)..Steps(2)).unwrap().is_empty());
    assert_eq!(live.pending_from(), Some(Steps(0)), "runner-up beam holds an undecided token inside MAX_LIVE_LAG");

    assert!(live.decode_live(&transducer, &hints, &window, Steps(2)..Steps(total)).unwrap().is_empty());
    assert_eq!(live.pending_from(), None, "once past MAX_LIVE_LAG, the silent winning beam clears pending_from");
}
