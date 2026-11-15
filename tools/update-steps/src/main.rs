//! Fails a change whose hub can't talk to the workers of the newest release published for a platform whose apps step,
//! since an app with those workers at work would have no release to step to, as "Updating the app" in
//! `docs/architecture.md` says. CI runs it for a change to the worker protocol, in the merge queue too, so a break there
//! waits until each such platform has published a release with the last one. It reads each feed where apps do.

mod platforms;

fn main() -> anyhow::Result<()> {
    platforms::leave_a_step(protocol::worker::HUB_SPEAKS)
}
