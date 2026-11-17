//! What the hub keeps of a repository that only a worker on its machine can read.

#![cfg(test)]

use protocol::model::{RepoOrigin, RepoUrl};

use crate::support::{Stack, git, wait_until};

/// One example of the wiring, from a checkout's config through the first worker that links: what a worker says and
/// what the hub keeps are each tested by their own crates.
#[test]
fn a_checkouts_address_is_read_by_the_first_worker_on_the_hubs_machine_without_its_credential() {
    let stack = Stack::start();
    git(&stack.repo(), &["remote", "add", "origin", "https://ada:ghp_made_up@example.invalid/ada/repo.git"]);
    let client = stack.client();
    let added = client.repo_add(stack.repo().to_str().unwrap()).unwrap();

    client.workspace_create(&added.id, None, None, None).unwrap();
    let url = || client.repo_list().unwrap().into_iter().find_map(|repo| repo.origin.url().cloned());
    wait_until("the checkout's address to be read", || url().is_some());

    assert!(matches!(added.origin, RepoOrigin::Checkout { .. }), "no worker had read it yet: {added:?}");
    assert_eq!(url(), Some(RepoUrl::try_from("https://example.invalid/ada/repo.git".to_owned()).unwrap()));
}
