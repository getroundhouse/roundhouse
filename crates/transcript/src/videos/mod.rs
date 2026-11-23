//! Videos the app plays: one an agent's text points to, which the hub kept, and one the file tab opens from the
//! worktree. Each is read from the hub a part at a time, as `video::Parts` asks, and decoded by `video::Player` on a
//! thread of its own. Its still is its first frame, which is all that's read of it until it's played, and a
//! [`VideoPlayer`] plays it, taking each frame as it's due and drawing it as an image, the same on every platform.

mod playing;
mod source;
mod still;

pub use playing::VideoPlayer;
pub(crate) use playing::expand;
pub use source::VideoSource;
pub use still::{Still, still};
