//! Lengths and places in a recording, counted in its samples.

/// The rate every recording is heard at, in samples a second: 16 kHz, the rate the models take. The microphone's own
/// is converted to it as it's heard.
pub const SAMPLE_RATE: u32 = 16_000;

/// A count of samples at [`SAMPLE_RATE`]: a length, or a place in a recording counted from its start.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Samples(pub usize);

impl Samples {
    pub const SECOND: Self = Self::from_secs(1);

    /// How many samples last `ms` milliseconds.
    pub const fn from_millis(ms: usize) -> Self {
        Self(ms * (SAMPLE_RATE as usize) / 1000)
    }

    pub const fn from_secs(secs: usize) -> Self {
        Self(secs * SAMPLE_RATE as usize)
    }
}

impl std::ops::Add for Samples {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl std::ops::Sub for Samples {
    type Output = Self;

    /// How far `other` is before `self`. Panics if it's after, which a caller has to have ruled out.
    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}
