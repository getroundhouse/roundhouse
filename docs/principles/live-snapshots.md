# Live snapshots

These rules cover live snapshots: what one is, and how one reaches a client. `docs/principles/serving.md` says what else a hub serves, and how a client follows a session or a terminal.

## What a live snapshot is

- **A live snapshot is what something running shows now: a reply as its agent writes it, or a terminal's screen.** A client holds the whole of it, and takes each change to it.

  ```rust
  // crates/protocol/src/tail/stream.rs, abridged
  pub struct StreamDelta {
      /// The launch of the agent that wrote it. The hub sets it, and only passes on the current run's.
      pub run: Option<RunId>,
      pub message_id: MessageId,
      pub change: DeltaChange,
  }

  pub struct Tail {
      /// More than one message can be open at once, because subagents stream alongside the main
      /// thread.
      pub messages: Vec<TailMessage>,
      /// The agent is trying a request to its model again, which failed, as it last said.
      pub retrying: Option<Retrying>,
  }

  // crates/protocol/src/terminal/picture.rs, abridged
  pub struct TerminalPicture {
      /// The rows the receiver lacks, by number, in runs of neighbours.
      pub rows: Vec<RowRun>,
      /// With this frame, the receiver has the screen as it was at one moment.
      pub whole: bool,
      /// Sent from nothing, as to a receiver that has just begun to watch, on each frame of the picture. The receiver
      /// lets go of all it holds of the terminal first, and of any picture it was gathering, since what came before it
      /// was for a watch that has ended, which the frames of a picture can't otherwise say.
      pub fresh: bool,
  }
  ```
- **A live snapshot is worth nothing once what it shows has moved on, which is what tells it from an observation.** A workspace's lines added and removed from five minutes ago are worth showing as five minutes old, and a terminal's screen from then isn't. The hub holds the newest of each in memory, only to send it whole to a client that begins to follow.
- **A live snapshot ends by becoming an event, or with what it shows.** A reply's ends when the finished message is recorded: the event arrives and replaces it. A terminal's ends when its shell does.

## Sending one

- **A live snapshot isn't stored, and a client that attaches or falls too far behind gets a new one, whole.** Nothing then queues for a slow client without limit. For example, a client that attaches in the middle of a message gets a snapshot of the tail and then the deltas, and the finished message arrives as an event and replaces them. A client on a slow link is sent a terminal's screen as it is now, rather than all that came between.
- **Only the current run's live snapshot reaches clients.** Each launch of an agent has a run ID, which every event and delta carries, and a later launch always has a higher one. The hub drops any other run's live snapshot. A late event from a replaced run still goes in the log, but it can't change the session's status or clear its replacement's tail.
- **A client has nothing to tell two copies of a live snapshot apart by, and needs nothing.** A live snapshot belongs to one run of an agent or one terminal, and arrives in order on one connection. To recover, a client replaces it with a new one, whole.
