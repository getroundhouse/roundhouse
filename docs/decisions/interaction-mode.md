# How the person talks with a chat's agent is its interaction mode, which every event is recorded under

**Status:** Decided. Partly built: the mode on the chat's record and on every event, and what the hub keeps true of it.
**First recorded:** 2026-11-20, in #2103.

A chat's record says its interaction mode: typing, as every chat is until conversation mode is turned on, or conversation, with the device that has the conversation. It's separate from the agent's own modes, such as plan mode and its permission mode, which say what the agent may do rather than how the person talks with it. The hub records every event with the mode the chat's record was in once the event had changed it, so the rule that turns conversation mode on is the first event of the conversation, and the prompt typed that turns it off is typed.

The hub keeps the mode true to what happened. A prompt typed and sent turns conversation mode off in the commit that records it. A prompt said out loud is taken only from the device that has the conversation, whether it's sent or queued. A prompt that waited in the queue leaves the mode as it is, since nobody sent it just then, and one said out loud that waited until the mode was off goes as typed, since nobody's listening for a spoken reply. Devices are the same by their IDs, so a renamed device keeps its conversation.

The rest of conversation mode reads the mark, and isn't built: how far each reply was heard is kept for the replies recorded in the mode, a reply recorded in the mode and never spoken shows as not read out, and the agent's adapter and the bar know when to cover a wait.

## Chosen over

- **A mark on prompts alone,** as whether each was said out loud. A reply would say nothing of being part of a conversation, so a client would have to work it out from the prompt before it, and the prompts of a turn can differ.
- **Recording only where the mode starts and ends, and working out the rest.** A client reads a log by range, as `session-log-not-crdt.md` says, so it would have to read back to the last change to know the mode of what it has. Turning the mode off, or another device taking it over, records no event.
- **Tags on events,** of which conversation would be one. The mode is one of a few, and one at a time, which a tag doesn't say, and nothing else needs tags yet.
- **One of the agent's modes.** Plan mode and the permission mode are the agent's, change what it may do and reach it as settings. The interaction mode is the person's, changes how they talk with it, and works the same with every agent.
