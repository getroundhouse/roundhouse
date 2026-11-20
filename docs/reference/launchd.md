# launchd

What launchd does to a hub run as a launch daemon, and to the processes it starts, as found by experiment on macOS 27.2.1 on Apple silicon. [A hub on a machine of its own runs as a service](../decisions/hub-package.md), so its workers' survival across an update rests on these.

- **Restarting a job, with `launchctl kickstart -k`, kills the job's process and the processes in its process group,** such as a child started with `&` from a shell. A child in a session of its own, started with `setsid`, as a worker that outlives its hub is, keeps running.
- **Stopping a job, with `launchctl bootout`, does the same:** the job's process group goes, and a child in a session of its own keeps running.
- **A launch daemon with `UserName` runs as that user,** in launchd's system domain rather than a login session's.
- **A launch daemon can run microVMs through Hypervisor.framework.** `msb run alpine -- uname -a`, from microsandbox 0.7.16, booted Linux and exited 0 from a launch daemon running as a user who isn't an administrator. The binary that runs the VM is signed with the `com.apple.security.hypervisor` entitlement, and `com.apple.security.cs.disable-library-validation` to load `libkrunfw`. That user was also logged in at the console, so a machine with nobody logged in wasn't tried.
- **A sandbox that `msb create` boots from a launch daemon outlives the job.** Its VM process, `msb machine`, leads a process group of its own, and kept running through `kickstart -k` and `bootout`.
