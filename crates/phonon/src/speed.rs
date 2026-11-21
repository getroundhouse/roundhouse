//! How fast the voice speaks, which Phonon takes as how much less time it gives each sound.

/// How fast the voice speaks. Normal unless chosen. Phonon speaks faster by giving each sound less time, which it
/// does to about 1.4 times its own pace: past that it slows again, and far past it the speech breaks, so there's
/// nothing faster.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Speed {
    /// The model's own pace.
    #[default]
    Normal,
    /// About 1.2 times as fast.
    Faster,
    /// About 1.4 times as fast.
    Fastest,
}

impl Speed {
    /// Every speed, slowest first, as a choice of them lists them.
    pub const ALL: [Self; 3] = [Self::Normal, Self::Faster, Self::Fastest];

    /// What it's called, as a choice of them says it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Faster => "Faster",
            Self::Fastest => "Fastest",
        }
    }

    /// What it's kept as, such as `"faster"`.
    pub fn code(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Faster => "faster",
            Self::Fastest => "fastest",
        }
    }

    /// The speed kept as `code`, or `None` for one this build doesn't know.
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|speed| speed.code() == code)
    }

    /// Phonon's `duration_delta`, which shortens each sound as it goes below 0. Measured on one sentence: -0.25 took
    /// 0.81 of its time at 0, and -0.5 took 0.70, while -0.75 took longer again, and -1.25 ran on to the longest the
    /// model makes.
    pub(crate) fn duration_delta(self) -> &'static str {
        match self {
            Self::Normal => "0.0",
            Self::Faster => "-0.25",
            Self::Fastest => "-0.5",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Speed;

    #[test]
    fn each_is_kept_by_its_code_and_is_faster_than_the_one_before() {
        for speed in Speed::ALL {
            assert_eq!(Speed::from_code(speed.code()), Some(speed));
        }
        assert_eq!(Speed::from_code("ludicrous"), None);
        let deltas: Vec<f32> = Speed::ALL.iter().map(|speed| speed.duration_delta().parse().unwrap()).collect();
        assert!(deltas.windows(2).all(|pair| pair[1] < pair[0]), "{deltas:?}");
        assert!(deltas.iter().all(|delta| (-0.5..=0.0).contains(delta)), "where Phonon speeds up: {deltas:?}");
    }
}
