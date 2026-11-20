# Conversation mode talks with the chat's own agent

**Status:** Decided. Not built.
**First recorded:** 2026-11-20, in #2075.

Talking with an agent out loud is a mode of an existing chat. What the person says reaches the chat's own agent as text, and what the agent writes is spoken. While the mode is on, the agent hands anything that takes more than a few seconds to subagents in the background, so it stays free to talk. When it can't answer at once, because it's busy or thinking, a small, fast model says what it's doing, in its voice and as the agent, and nothing else. The agent's adapter writes that line, with Haiku on the chat's own account, and reports it. The hub keeps it as the session's live state, every device shows it, and the device that has the conversation says it.

The conversation needs the judgement of the agent's model, such as Opus, and the agent that does the work should be the one that heard what was said. On ordinary turns, a warm Claude CLI on Opus starts answering about as soon as a person expects a reply out loud, so the agent can hold the conversation itself, and only the moments it can't answer need covering.

## Chosen over

- **A small, fast model holding the conversation and passing work to the agent.** It answers sooner, but the person talks with a weaker model, and the agent hears the conversation second-hand.
- **A conversation of its own, forked from the chat.** A fork starts answering quickly and doesn't disturb the chat it came from. But what's settled in the fork doesn't reach the agent doing the work, and the person has two agents to keep track of.
- **A conversational agent beside the chat, coordinating with it,** as some products pair a model that talks with one that works. That's two agents again, with a protocol between them, where an agent that hands out the work while it talks is one.
- **Silence while the agent is busy.** A spoken conversation that goes quiet for several seconds feels broken, and the agent is often busy for longer than that.
