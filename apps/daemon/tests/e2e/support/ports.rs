//! Where a stack's hubs listen for the workers that dial them: on loopback alone, at ports of their own.

use std::fs::{File, TryLockError};
use std::sync::atomic::{AtomicU16, Ordering};

use process::testing::WholeEnvironment;

/// Has a stack's hubs use no relay for the endpoint their workers dial, and bind only to loopback, which keeps a test
/// off the internet, at ports no other stack is given.
pub(super) fn off_the_internet(environment: &mut WholeEnvironment) {
    environment
        .set(protocol::testing::ENV_TEST_NO_RELAYS, "1")
        .set(protocol::testing::ENV_TEST_ENDPOINT_PORTS, endpoint_ports().to_string());
}

/// The ports a stack's hubs bind, as [`hub::EndpointPorts::for_a_test`] says, which no other stack is given, of this
/// run or of another running at once, as from another checkout. A hub of another test drawing from the same ports
/// could otherwise take the one a hub kept while it restarts, and an adapter's worker in a test, which has no relay to
/// fall back on and no address file to look the hub up in, would never find that hub again. Each block is claimed by a
/// lock on a file of its own in the system's temporary directory, held until the run ends, which the system lets go of
/// however the run ends. Below the ports a hub draws from otherwise, as a test's endpoints outside a stack do, and the
/// systems' ephemeral ports.
fn endpoint_ports() -> u16 {
    const PORTS: std::ops::Range<u16> = 10_000..20_000;
    static CLAIMED: AtomicU16 = AtomicU16::new(0);
    let per_stack = hub::EndpointPorts::FOR_A_TEST;
    let blocks = (PORTS.end - PORTS.start) / per_stack;
    let dir = std::env::temp_dir().join("roundhouse-test-ports");
    std::fs::create_dir_all(&dir).unwrap();
    // From a place of the run's own, so runs at once mostly try different blocks first.
    let from = u16::try_from(std::process::id() % u32::from(blocks)).unwrap();
    for tried in 0..blocks {
        let first = PORTS.start + (from + tried + CLAIMED.load(Ordering::Relaxed)) % blocks * per_stack;
        let lock = File::create(dir.join(format!("{first}.lock"))).unwrap();
        match lock.try_lock() {
            Ok(()) => {
                CLAIMED.fetch_add(1, Ordering::Relaxed);
                // Open, and so locked, until the run ends.
                std::mem::forget(lock);
                return first;
            }
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Error(e)) => panic!("claiming ports {first}: {e}"),
        }
    }
    panic!("every block of ports for a test's hubs is claimed")
}
