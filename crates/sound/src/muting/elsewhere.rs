//! Windows and Linux, which don't mute their sound output yet, so [`MUTES_OUTPUT`] is false and the app doesn't offer
//! to.

/// Whether this platform can mute its sound output while something is heard. Windows and Linux can't yet.
pub(crate) const MUTES_OUTPUT: bool = false;

/// Never made: nothing is muted here.
pub(crate) enum MutedOutput {}

/// Mutes nothing, as [`MUTES_OUTPUT`] says, so what's playing goes on playing while it's dictated to.
pub(crate) fn mute_output() -> Option<MutedOutput> {
    None
}

/// Answers at once, since nothing is muted here.
pub(crate) fn output_restored() -> flume::Receiver<()> {
    let (answer, answered) = flume::bounded(1);
    let _ = answer.send(());
    answered
}
