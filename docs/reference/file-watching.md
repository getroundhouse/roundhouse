# Watching files

What the OS's file watchers do through `notify`, which a worker uses to watch a worktree's branches and its changes, as found by experiment or read from notify 8.2.2's source. `crates/worker/src/changes_watch/` and `crates/worker/src/branch_watch/` depend on it. The measurements behind the choice of watch on each platform are in #2393.

## Linux

- **inotify keeps one watch for each folder, whatever name it was watched by,** so a folder watched under a link's name as well as its own is told of under one of them. notify follows links to folders unless told not to (`Config::with_follow_symlinks(false)`). On a shallow clone of the Linux kernel's tree, watched whole with links followed, 2 of 50 edits came back named under `scripts/dtc/include-prefixes/dt-bindings/`, a link to `include/dt-bindings/`, rather than under the file edited. With links not followed, and with a watch on each folder, none of the 50 was missed or named otherwise. Seen on kernel 6.12 and git 2.39.
- **A user's inotify watches are shared by all their processes,** up to `/proc/sys/fs/inotify/max_user_watches`, which a stock kernel sets from the machine's memory: 128,312 on a 16 GB VM. A watch on each folder of a 300,000-file `node_modules` takes some 150,000 of them, so a worker watches only the folders git doesn't ignore, and gives a worktree at most half of the limit.
- **A file written into a folder just made can come before the folder's watch,** which nothing then tells of. So a new folder means a new summary, which git reads in full.
- **A file's close is told of once the last process holding it closes it,** so it can be told of after what happened later. A process forked while the file was open, as when another thread starts git, holds it until it starts its program, since Rust opens files to close on exec. In the worker's watch tests, run six at a time on kernel 6.12, one file's `IN_CLOSE_WRITE` came after a folder made once the file was written, in 1 of 36 runs, which that would explain.

## macOS

- **FSEvents tells of a whole tree for about the same at any size,** about 2 ms and 1 MB to start, and notify puts every folder it's given into one stream, checking each event against each, so a watch of thousands of folders one by one costs more per event, and past about 7,000 doesn't start. FSEvents names paths with every link followed, so `/tmp` is `/private/tmp`, and a watch compares what it's told with the worktree's path canonicalized.

## Windows

Read from notify 8.2.2's source, and not run.

- **notify gives each watch a buffer of 16 KB for `ReadDirectoryChangesW`,** and doesn't look at how many bytes a completion returned, so a buffer that overflowed isn't told of as a rescan. An error it doesn't know, such as `ERROR_NOTIFY_ENUM_DIR`, it logs, and ends that watch without telling the program. So a worker watches each folder at the top of a worktree that git doesn't ignore on its own, rather than the top with all in it, and watches again at each read every few minutes.
