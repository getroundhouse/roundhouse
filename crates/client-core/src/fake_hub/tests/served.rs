use crate::Connection;

use super::super::{World, serve};

/// A client of a fake hub serving `world`, and the directory its socket is in, which goes when the test ends.
pub(super) fn served(world: World) -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub");
    serve(&socket, world);
    let client = Connection::connect(&socket, "test").unwrap();
    (dir, client)
}
