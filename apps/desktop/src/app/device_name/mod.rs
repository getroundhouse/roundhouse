mod name;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
use unix as system;
#[cfg(windows)]
use windows as system;

#[cfg(any(test, feature = "testing"))]
pub use name::the_device_name_is;
pub(crate) use name::{read_in_background, read_now};
pub(crate) use system::computer_name;
