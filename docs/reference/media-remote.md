# MediaRemote

What MediaRemote, the private framework that says what a Mac is playing, does, as found by experiment on macOS 27.2.1. `crates/now-playing` depends on each of these, and [the decision to pause through perl](../decisions/pause-media-through-perl.md) says why it's used.

- **MediaRemote answers only Apple's own programs about what's playing,** since macOS 15.4. Asked from any other process, `MRMediaRemoteGetNowPlayingApplicationPID` answers 0, `MRMediaRemoteGetNowPlayingApplicationIsPlaying` false, and `MRMediaRemoteGetNowPlayingInfo` an empty dictionary.
- **`/usr/bin/perl` is answered, and loads a library signed by anyone,** through `DynaLoader`. It's signed as `com.apple.perl`, with no hardened runtime, so it has no library validation. A library signed ad hoc loads, and so does one signed with the hardened runtime.
- **Perl loads a library only by its absolute path.** Given a relative one, `dlopen` refuses it with "relative path not allowed in hardened program".
- **Perl is built for `x86_64`, `arm64e` and `arm64e.x1`, and loads a library built for `arm64`,** as Rust builds one for Apple silicon.
- **`MRMediaRemoteSendCommand` works from any process,** for pause (1) and play (0). It returns at once, and sends the command later, so a process that ends first loses it. A question asked after it, such as `MRMediaRemoteGetNowPlayingApplicationPID`, is answered only once the command has gone, so waiting for that answer is enough.
- **A run of perl that loads a library and asks what's playing takes about 0.2 s.**
