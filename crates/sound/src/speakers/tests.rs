//! What the speakers' device thread plays, and when it says each mark is reached, without a device.

use super::queue::Queue;

/// What one callback of `frames` frames in `channels` channels plays, and the marks it reaches.
fn play(queue: &mut Queue, frames: usize, channels: usize) -> (Vec<f32>, Vec<u64>) {
    let mut out = vec![9.0; frames * channels];
    let mut reached = Vec::new();
    queue.fill(&mut out, channels, |sample| sample, &mut |mark| reached.push(mark));
    (out, reached)
}

/// Each sample goes to every channel of its frame, in order, and silence follows once there's nothing left.
#[test]
fn each_sample_plays_in_every_channel_and_silence_follows() {
    let mut queue = Queue::default();
    queue.give(&[0.1, 0.2, 0.3]);

    let (played, _) = play(&mut queue, 4, 2);

    assert_eq!(played, [0.1, 0.1, 0.2, 0.2, 0.3, 0.3, 0.0, 0.0]);
    assert_eq!(play(&mut queue, 1, 2).0, [0.0, 0.0]);
}

/// A mark is reached only once everything given before it has been played, and marks come in order, each once.
#[test]
fn a_mark_is_reached_once_everything_before_it_has_played() {
    let mut queue = Queue::default();
    queue.give(&[0.1; 3]);
    queue.mark(1);
    queue.give(&[0.2; 2]);
    queue.mark(2);
    queue.mark(3);

    assert_eq!(play(&mut queue, 2, 1).1, [] as [u64; 0], "one is still to play before the first");
    assert_eq!(play(&mut queue, 1, 1).1, [1]);
    assert_eq!(play(&mut queue, 1, 1).1, [] as [u64; 0]);
    assert_eq!(play(&mut queue, 5, 1).1, [2, 3], "two marks with nothing between come together");
    assert_eq!(play(&mut queue, 5, 1).1, [] as [u64; 0], "once");

    queue.mark(4);
    assert_eq!(play(&mut queue, 1, 1).1, [4], "nothing was given before it that hasn't played");
}

/// Stopping drops what's still to play and its marks, which are never reached, and what's given after plays as
/// before.
#[test]
fn stopping_drops_whats_left_and_its_marks() {
    let mut queue = Queue::default();
    queue.give(&[0.1; 4]);
    queue.mark(1);
    play(&mut queue, 2, 1);

    queue.clear();
    assert_eq!(play(&mut queue, 4, 1), (vec![0.0; 4], vec![]));

    queue.give(&[0.3]);
    queue.mark(2);
    assert_eq!(play(&mut queue, 2, 1), (vec![0.3, 0.0], vec![2]));
}
