//! The accounts agents run under, since the floor.

/// How much of each account's week the person keeps for themselves, from 1 to 100 percent, or `NULL` for none, which
/// its record shows. Only an account whose login a person added has one.
pub(super) const RESERVES: &str = r"
    ALTER TABLE accounts ADD COLUMN reserve_percent INTEGER;
";

/// Who each account is, which the hub keeps as an observation, in its cache file, and no longer here. Nothing of it is
/// carried there: a worker reads it again after the next launch on the account's login, and an account whose login
/// has gone from every machine has nothing to run on.
pub(super) const IDENTITY_OBSERVED: &str = r"
    ALTER TABLE accounts DROP COLUMN model_provider;
    ALTER TABLE accounts DROP COLUMN plan;
    ALTER TABLE accounts DROP COLUMN login;
    ALTER TABLE accounts DROP COLUMN email;
";

/// What's left of each account's limits, which the hub keeps as an observation, in its cache file, and no longer here.
/// The table's triggers, which moved the account's revision as a limit changed, go with it. Nothing of it is carried
/// there: each account's limits are known again once an agent under it next says them.
pub(super) const LIMITS_OBSERVED: &str = r"
    DROP TABLE account_limits;
";
