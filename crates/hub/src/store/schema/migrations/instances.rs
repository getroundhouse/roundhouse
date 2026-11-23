//! What the hub keeps of each instance it asks a provider for, since the floor. Each has its place in `MIGRATIONS`.

/// The secret of the ticket the hub last gave the instance's provider, until a worker dials with it, and the key of the
/// worker it admitted, which it admits again by that key alone, as `protocol::dialing` says. NULL for neither.
pub(super) const WORKER_TICKETS: &str = r"
    ALTER TABLE instances ADD COLUMN ticket TEXT;
    ALTER TABLE instances ADD COLUMN worker_key TEXT;
";
