# A workspace's pull request is found from the branch it's pushed to, with no stored link

**Status:** Decided.
**First recorded:** 2026-11-17, in #1785.

The hub finds a workspace's pull request by asking GitHub for the pull requests from one branch: the branch on GitHub that the workspace's branch is pushed to. It works that branch out from the workspace's record, which has the branch checked out, its upstream, and the repository the upstream's remote names.

A branch on GitHub belongs to one repository under one name, and a pull request is from that branch. A branch on a machine has whatever name that machine gives it, and several machines can use the same name. So no workspace owns a pull request, and workspaces checked out on the same branch show the same one.

## Chosen over

- **Storing a link from a branch to its pull request.** Something would have to keep the link correct as branches are renamed, deleted and made again.

## What it costs

Once the branch is gone from GitHub, as when GitHub deletes it after a merge, the hub has to ask by the branch's name, which also lists every fork's branch of that name. The hub looks at only the five newest pull requests of each list, so a name that many forks use can crowd out the branch's own.
