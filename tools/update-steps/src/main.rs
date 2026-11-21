//! Fails a change that would leave an app with no release to step to, as "A break in the worker protocol" in
//! `docs/standards/merging.md` says: one whose hub can't talk to the workers of the newest release published for a
//! platform whose apps step, and one whose build updates only from a release some platform's apps can't reach. CI runs
//! it for a change to the worker protocol, or to `crates/update-feed`, where a build's floors are, in the merge queue
//! too, so such a change waits until each platform has published the release it needs. It reads each feed where apps
//! do, since a merge isn't always published: the release workflow passes over a build that hasn't started once a newer
//! merge comes.

mod platforms;

fn main() -> anyhow::Result<()> {
    platforms::leave_a_step(update_feed::FLOORS)
}
