use crate::Client;

use super::super::{World, serve};

/// A client of a fake hub serving `world`, and the directory its socket is in, which goes when the test ends.
pub(super) fn served(world: World) -> (tempfile::TempDir, Client) {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub");
    serve(&socket, world);
    let client = Client::connect(&socket, "test").unwrap();
    (dir, client)
}
