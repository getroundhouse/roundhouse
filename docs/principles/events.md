# Events

These rules cover a session's events: what one is, how the hub records it, and how a client reads a log. `docs/principles/serving.md` says what else a hub serves, and how a client follows a session.

## What an event is

- **An event is something that happened in a session, and a session's events, in order, are its log.** A prompt, a message the agent wrote, a tool call and a turn ending are each one. Its `seq` is its position in the log, and its place is its position among the events from the same agent or subagent. Neither has gaps.

  ```rust
  // crates/protocol/src/event/recorded.rs, abridged
  pub struct SessionEvent {
      pub session_id: SessionId,
      pub seq: Seq,
      /// The launch of the agent that produced it. `None` for what the hub records itself, and for
      /// events recorded before runs were.
      pub run: Option<RunId>,
      pub subagent: Option<SubagentId>,
      pub place: Place,
      pub body: EventBody,
  }
  ```
- **A session's log has one writer, its hub.** So its events get a plain sequence number, `seq`, and the log needs no merge logic.
- **A log's events don't change once recorded, except in a migration.** A migration can rewrite events stored in an older form, and a rewritten event keeps its `seq` and place. So a client that reaches a hub of another build reads the log again. A hub migrates before it replies to a hello, so no client reads a log part way through a migration.
- **A value where only the newest reading matters goes on a record, in an observation or in a live snapshot, and isn't an event.** Nothing that reads the log would use the older readings. For example, how full an agent's context is is an observation, and an agent's retry of a failed model request is in the tail. *Not built:* how full a context is is on the session's record.

## Recording one

- **A prompt or an approval is real only once the hub has recorded it.** Until then a client shows it as pending. A session's settings follow the same rule: a change to the model, approval mode or effort goes to the running agent first, and the hub saves it only when the agent accepts it. So a record is never ahead of what's true.
- **The hub records every event an adapter sends it, and the client decides whether to show an event.** An event's place in the log records which session or subagent it belongs to, not whether it's worth showing. For example, `session_find` returns which events hold the text, and the client counts the matches in what those events render.
- **What only the hub can say of an event, the hub adds to the event as it records it.** A client then reads it from the event, and a hub that starts again reads it from the log. For example, the hub adds when a turn started to the event that ends it, and whether a failed request repeats the one before to the event that records the failure. *Unconfirmed.*
- **What a session's log opens, it closes once, and the hub closes it where the agent can't.** The agent's adapter records how a prompt, an approval or a run ended. When the agent can't, such as when its worker is lost, the hub records it. So a client reads how each ended from the log, and never works it out from what came after. *Unconfirmed.*
- **The hub stores everything the transcript renders, as it was when the event was recorded, before it records the event.** The transcript then doesn't need the worker, shows the same on every client, and keeps its images after a restart and after the workspace is archived. A client also never requests something the hub doesn't have. For example, the worker reads an image that an agent's reply links to as the finished message passes through, and the hub stores it as an attachment before it records the message. If the agent later changes the file, the reply still shows the earlier image.

## Reading a log

- **A client keeps events in order by `seq`, or by place, and after a reconnect it follows the session again from one past the last it has.** It drops one it already has, so nothing is lost or doubled however the connection went.
- **A subscriber reads events from the log, and a live update only signals that the log advanced.** A slow client falls behind and catches up without losing anything, and it delays only itself.
- **A client holds only a window of a long session's events, so anything about the whole session is a request to the hub.** The hub reads only the event kinds and fields the request needs, so the cost doesn't grow with the rest of the session. For example, `session_prompt_list` reads only prompts and the events that take or drop them.
