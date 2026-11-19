//! What a host does once the last words dictated are in, for a host whose submitting key can do more than one thing,
//! such as the composer's Enter, which sends or queues. A host with one such action does it on
//! [`Event::Ended`](crate::Event::Ended) asking to submit, which only the host's own asking makes it.

use gpui_kit::{App, Entity};

use crate::Field;

/// The action a host asked for while dictating, which waits for the recording to end. It's held only while a
/// recording is underway, and however that ends, [`Pending::ended`] lets go of it, so ending or hiding the field needs
/// nothing more of the host.
pub struct Pending<T>(Option<T>);

impl<T> Default for Pending<T> {
    fn default() -> Self {
        Self(None)
    }
}

impl<T> Pending<T> {
    /// Whether `then` waits for the last words dictated into `field`, as [`Field::finish_before_submit`] says, with
    /// `can_submit` whether the host could act now. It's kept until the recording ends.
    pub fn wait(&mut self, field: &Entity<Field>, can_submit: bool, then: T, cx: &mut App) -> bool {
        let wait = field.update(cx, |field, cx| field.finish_before_submit(can_submit, cx));
        if wait {
            self.0 = Some(then);
        }
        wait
    }

    /// The action to take now that the recording has ended, if it ended asking to submit, letting go of it either way.
    pub fn ended(&mut self, submit: bool) -> Option<T> {
        self.0.take().filter(|_| submit)
    }
}
