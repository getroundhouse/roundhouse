# A spoken prompt reaches the agent with how to talk, added by its adapter

**Status:** Decided. Not built. Partly replaced by [interaction-mode.md](interaction-mode.md): every event, a prompt included, says the mode it was recorded under.
**First recorded:** 2026-11-20, in #2075.

The hub records with each prompt whether it was said out loud. Whether the mode is on, and which device has the conversation, are on the chat's record, and a prompt carries nothing more about them. The agent's adapter puts a short instruction in front of every spoken prompt as it delivers it: talk briefly and for the ear, hand anything that takes more than a few seconds to a subagent in the background, and say briefly when one finishes. The instruction's text lives once, in the worker. Typing turns conversation mode off, so the prompts sent while it's on are exactly the spoken ones.

So it works in any chat, whenever the mode is turned on, it survives a compaction, and hearing it with every prompt keeps the agent brief as a conversation grows. The transcript shows the mode's rule across the chat in its place, and the prompt as it was said.

## Chosen over

- **The system prompt.** The Claude CLI keeps a chat's first system prompt until the chat is compacted, so it couldn't be turned on partway through a chat, and the instructions every agent gets stay the same whatever it's doing, as `docs/principles/agents.md` says.
- **Once, as the mode turns on.** A compaction would lose it, and an agent drifts back to long, written replies as a conversation grows.
- **A flag for the mode on each prompt, with typed prompts in the mode spoken too.** It needs two facts on a prompt rather than one, and makes what the mode means harder to tell: whether a reply is spoken would depend on a setting rather than on how the prompt was sent.
