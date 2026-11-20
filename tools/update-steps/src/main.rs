//! Fails a change whose hub can't talk to the workers of the newest release published for a platform whose apps step,
//! since an app with those workers at work would have no release to step to, as "A break in the worker protocol" in
//! `docs/standards/merging.md` says. CI runs it for a change to the worker protocol, in the merge queue too, so a break
//! there waits until each such platform has published a release with the last one. It reads each feed where apps do,
//! since a merge isn't always published: the release workflow passes over a build that hasn't started once a newer
//! merge comes.

mod platforms;

fn main() -> anyhow::Result<()> {
    platforms::leave_a_step(protocol::worker::HUB_SPEAKS)
}
