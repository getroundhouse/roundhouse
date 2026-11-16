//! Backpressure never blocks capture or shutdown, and never silently loses part of a recording.

use crate::DictationError;
use crate::dictation::buffer::{Buffer, MAX_SAMPLES};
use crate::dictation::recording::Input;

#[test]
fn stopping_a_full_buffer_keeps_its_audio_in_order_and_refuses_later_audio() {
    let buffer = Buffer::new();
    buffer.audio(vec![0.1; MAX_SAMPLES - 1]);
    buffer.audio(vec![0.2]);

    buffer.end(Input::Stop);
    buffer.audio(vec![0.3]);

    assert!(!buffer.accepting());
    assert!(matches!(buffer.next(), Input::Audio(audio) if audio == vec![0.1; MAX_SAMPLES - 1]));
    assert!(matches!(buffer.next(), Input::Audio(audio) if audio == [0.2]));
    assert!(matches!(buffer.next(), Input::Stop));
}

#[test]
fn consumed_audio_releases_capacity_and_overflow_discards_the_backlog() {
    let buffer = Buffer::new();
    buffer.audio(vec![0.1; MAX_SAMPLES]);
    assert!(matches!(buffer.next(), Input::Audio(audio) if audio.len() == MAX_SAMPLES));
    buffer.audio(vec![0.2; MAX_SAMPLES]);
    assert!(buffer.accepting());

    buffer.audio(vec![0.3]);

    assert!(!buffer.accepting());
    assert!(matches!(buffer.next(), Input::Failed(DictationError::AudioBacklog)));
}
