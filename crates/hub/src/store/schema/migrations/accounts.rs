//! The accounts agents run under, since the floor.

/// How much of each account's week the person keeps for themselves, from 1 to 100 percent, or `NULL` for none, which
/// its record shows. Only an account whose login a person added has one.
pub(super) const RESERVES: &str = r"
    ALTER TABLE accounts ADD COLUMN reserve_percent INTEGER;
";
