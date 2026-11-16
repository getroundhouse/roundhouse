//! Multiplying rows by a weight matrix and adding its bias: through Accelerate's BLAS on a Mac, so even unoptimized
//! debug builds run in microseconds, and in portable Rust elsewhere.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(any(not(target_os = "macos"), test))]
mod portable;

#[cfg(target_os = "macos")]
pub(super) use macos::linear_rows;
#[cfg(not(target_os = "macos"))]
pub(super) use portable::linear_rows;

#[cfg(all(target_os = "macos", test))]
mod tests;
