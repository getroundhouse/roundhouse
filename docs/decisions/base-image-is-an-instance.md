# A base image is an instance, built through a worker

**Status:** Decided, not built.
**First recorded:** 2026-11-11, when the provider protocol was settled. Written down in #2002.

A base image is an ordinary instance that has been set up and then turned into something other instances are copied from. The hub creates a plain instance and has its worker clone the repository and run the repository's base image script, the way it runs a setup script, with a log. The hub then stops the instance and asks the provider's adapter to make it a base image, with `make_base_image`. For Incus that's a snapshot, and for EC2 it's `CreateImage`. A base image can't start again.

The hub keeps a record of each base image: its repository, its provider, the commit, and a hash of the script. The newest one that finished building is the current one, and a new workspace is a copy of it. For each repository and provider the hub keeps the current one and the one before it, so there's one to go back to, and destroys older ones.

`make_base_image` is in the provider protocol. The rest isn't built: no script runs, and the hub keeps no record of a base image.

## Chosen over

- **Base images as a thing of their own in the provider protocol,** which an adapter builds from a script, lists and deletes. The adapter would have to run the script, keep its log and hold the credentials it needs. Built through a worker, all three go through the worker protocol, which already carries them for setup scripts, and an adapter only makes, copies and removes VMs. The protocol also keeps no list of images, since the hub's records say which exist.

## What it costs

A provider has to let the hub destroy a base image while workspaces copied from it still run, or the number kept isn't bounded. Incus keeps what its ZFS clones depend on until they're gone, and an EC2 instance doesn't need its AMI.
