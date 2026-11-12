# The Incus provider

`roundhouse-incus` is the Incus provider's adapter. A hub runs it and speaks the provider protocol to it on its stdin and stdout, as "Providers" in `docs/architecture.md` describes. Each workspace runs in a VM of its own on an Incus server, copied from a base image's snapshot named `base`, which on ZFS is a clone and takes about a second. Its worker's stream is `incus exec`, which the hub runs itself. The adapter drives the `incus` CLI, so it uses the remotes `incus remote add` set up, and the hub holds no secret for it.

It's a proof of concept, so some of what the hub should do is set up by hand for now:

- **The base image comes from `--base-image`,** a VM you set up, until the hub builds one per repository with its script (#597).
- **The worker and the agent CLI are already in the image,** until the hub puts its own worker in place as an instance starts (#600). Pass the agent with `--env ROUNDHOUSE_CLAUDE_BIN=...`.
- **The repository is in the image at the path the hub recorded,** since worker requests carry the hub's paths (#514).
- **Stopping a worker leaves its VM running,** as stopping does on every provider, and ends everything running as the worker's user. Putting an idle VM to sleep is a request of its own, which is yet to come (#627). Archiving leaves the VM too, since its branch is only there (#628).

## Setting it up

1. **Give Roundhouse a project of its own,** with limits on CPU, memory and instances, so workspaces can't starve what else runs on the server: `incus project create <remote>:roundhouse -c limits.instances=6 -c limits.cpu=24 -c limits.memory=48GiB`. Its `default` profile is its own and starts empty, so give it a root disk and a network: `incus --project roundhouse profile device add <remote>:default root disk path=/ pool=<pool>` and `incus --project roundhouse profile device add <remote>:default eth0 nic network=<network>`. The commands below are all in that project. A workspace the limits leave no room for fails as one with no room, rather than as an error.
2. **Make the base image,** a VM in that project with:
   - a user for the worker, which runs nothing else, since everything running as it is ended with the worker, whose IDs you pass with `--user` and `--group`, and its home, passed with `--home`;
   - a Rust toolchain in that home, and the repository built once, so a workspace starts with a warm build;
   - `roundhoused` at `--worker`, built for Linux from the same commit as the hub, and the agent CLI;
   - the state directory at `--state-dir`, owned by the worker's user;
   - `/etc/machine-id` emptied, so each copy makes its own and gets its own address from DHCP;
   - only the `/etc/hostname` and `/etc/hosts` templates in its metadata, with `incus --project roundhouse config metadata edit <remote>:rh-base`. Ubuntu's cloud image also has templates that write cloud-init a new seed when the VM is copied, and the copy then reboots so cloud-init can run for it, which doubles its first boot, to about 18 s. Without them a copy boots once, in about 9 s, keeps the base image's cloud-init state and SSH host keys, and gets its own hostname.
3. **Stop it and take its snapshot:** `incus --project roundhouse stop <remote>:rh-base && incus --project roundhouse snapshot create <remote>:rh-base base`.
4. **Name the provider in the hub's data directory,** in `providers.json`, and make it the default for new workspaces:

```json
{"default": "incus", "providers": [{"name": "incus", "program": "/path/to/roundhouse-incus", "args": [
  "--incus", "/opt/homebrew/bin/incus", "--remote", "<remote>", "--project", "roundhouse", "--base-image", "rh-base",
  "--user", "1002", "--group", "1002", "--env", "ROUNDHOUSE_CLAUDE_BIN=/usr/local/bin/claude"]}]}
```

The hub reads it as it starts. A provider whose adapter won't start stays, and its workspaces fail with why until it's fixed. `roundhouse-incus --help` lists the rest of the flags.

Each VM is named `rh-` and its instance's ID, and tagged with its hub's ID, so `incus list <remote>: --project roundhouse` shows them, and one server can serve several hubs.
