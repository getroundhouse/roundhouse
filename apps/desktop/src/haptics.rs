//! What's felt through a Mac's trackpad. macOS plays it on a Force Touch trackpad, going by its accessibility settings
//! and the person's preferences, and a mouse feels nothing. Elsewhere there's nothing to play it on.

/// Taps as macOS does when something dragged lines up with a guide, as Keynote does at one, so a drag that settles on
/// a place can be felt doing it.
pub(crate) fn align() {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{
            NSHapticFeedbackManager, NSHapticFeedbackPattern, NSHapticFeedbackPerformanceTime,
            NSHapticFeedbackPerformer as _,
        };
        // At once, rather than at what AppKit takes for the next screen update, since GPUI draws through Metal on its
        // own. What was dragged moves in the frame being asked for, so the two still land together.
        NSHapticFeedbackManager::defaultPerformer().performFeedbackPattern_performanceTime(
            NSHapticFeedbackPattern::Alignment,
            NSHapticFeedbackPerformanceTime::Now,
        );
    }
}
