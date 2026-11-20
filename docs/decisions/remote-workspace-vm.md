# A remote workspace runs in a VM of its own, not a container

**Status:** Decided, not built.
**First recorded:** 2026-11-11, in #604.

Each remote workspace runs in its own VM, made from its repository's newest base image and destroyed when the workspace is archived. A base image is a VM that a script in the repository has set up, with its tools installed and its caches warm, so a new workspace starts with a warm build.

## Chosen over

- **A container.** A VM is as flexible as a real machine. An agent can run containers in it, and the services and kernel features its project needs, which are hard to get inside a container. A VM is also a stronger boundary between workspaces whose agents run unattended with credentials.
