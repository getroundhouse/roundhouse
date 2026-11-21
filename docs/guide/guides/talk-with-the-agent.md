# Talk with the agent

Talk with a chat's agent out loud, and hear it answer, while it goes on working. It's the chat's own agent, so the one doing the work is the one that heard you. What you say is heard on your Mac, and its replies are spoken there, so no sound leaves it. Only the words reach the agent.

Conversation mode hears you with [dictation](dictate-a-prompt.md), and reads the agent's replies out with speech. Each downloads once, dictation's model 2.4 GB and speech's voice about 65 MB, and each is a setting of its own in [Settings](../reference/settings.md#general).

## Turn it on

Click the button between the context's ring and the microphone, or press `Cmd-Shift-Space`. A slim bar takes the composer's place, and a rule across the chat says when the mode turned on.

The first time, a card opens instead, listing dictation and speech and how each stands. **Turn on both**, or whichever isn't on, downloads what's needed, and the card shows how far each has got. Once both are ready, the mode turns on by itself. **Not now** closes the card and leaves the mode off.

## Talk

| Key | What it does |
|---|---|
| Space, held | Listens while you hold it, and sends what you said when you let go. Holding it while the agent speaks talks over it, which stops the voice |
| Escape | While you hold Space, drops what you said and sends nothing. While the agent speaks, stops the voice |
| Any other key | Leaves the mode and brings the composer back, with what you typed in it |

While you hold Space, the bar shows what it hears. Once that's more than fits, it shows the newest words, and the chat has all of it once it's sent.

While the agent is busy or still thinking, a small model says what it's doing, in a sentence in the agent's voice, such as "Hold on, I'm writing the brief for the parser agent." It never answers for the agent.

The agent's replies are read out whole, with Markdown's marks taken out, so code and a table's cells are read as they're written. The chat shows how far each reply was heard, dimming what wasn't, and the line at the end of a turn says why its last reply was cut off: **Talked over** when you talked over it, or **Stopped** when Stop or typing stopped it.

Nothing is read over you while you talk: a reply that comes while you're talking, or before the agent has taken what you said, is skipped, since you've moved on. Press Escape to drop what you said, and the next reply is read.

The conversation goes on while you look at another chat, another workspace or Settings, as long as its chat's tab is open. Holding Space talks only where the chat's bar shows. Closing the chat's tab leaves the mode.

## Leave it

**Type**, or any key you type, leaves the mode with the composer back, and anything you'd typed before is still there. ✕ leaves it too. While the agent works, Stop takes ✕'s place, and stops the agent as Send's Stop does.

If the hub can't turn the mode off, the app says why along the top, and the conversation carries on as before. ✕ or **Type** asks the hub again.

## On another Mac

The chat is everyone's, and the conversation is one Mac's. Another Mac showing the chat says where the conversation is, such as "The conversation is on Mac Studio", and still shows what the agent is doing and when it's speaking, quietly. **Take over** moves the conversation there, and the Mac it left says where it went, with **Take back**.
