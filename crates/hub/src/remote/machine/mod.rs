#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub(crate) use unix::this_machine;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(crate) use windows::this_machine;
