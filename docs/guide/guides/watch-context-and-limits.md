# Watch context and limits

Three things run out in a long chat: the agent's context, your plan's limits, and the prompt cache. The composer shows the first two, and the chat says when the third is about to cost you.

## How full the context is

A ring beside Send fills as the agent's context does. Rest the pointer on it for a card that says:

- **How much is used,** such as `63% used`.
- **How much more it can take** before the agent compacts, such as `153.5k until it compacts`.
- **What's in it,** largest first.

![The context's card, open above its ring](../../spec/screens/context.png)

## Compact when you choose

When a context fills, the agent compacts by itself: it swaps what it holds for a summary, and carries on. You can do that sooner, at a moment that suits the work:

- **`Compact now`,** on the ring's card.
- **`/compact`,** typed in the composer. What follows it says what to keep, such as `/compact keep the failing test's output`.

A compaction shows in the chat as a rule, with how many tokens the context went from and to. Click it to read the summary the agent carries on from.

![A compaction someone asked for, and the turn after it](../../spec/screens/compacted.png)

It's refused while the agent is on a turn.

## What's left of your plan

Bars left of the ring show your account's limits, one for each, emptying as the limit is used. Rest the pointer on them for how much of each is left, and when it starts again.

![The limits' card, open above their bars](../../spec/screens/usage.png)

A limit is your account's, not the chat's, so every chat under the same login shows the same bars. Under them, the card says whose account it is.

## When the prompt cache expires

The model provider keeps what an agent's requests have read for a while, so the next request needn't send it all again. On a Claude account that lasts an hour from the agent's last request.

A prompt sent after the cache has expired writes the whole conversation again, which in a long chat is the most a prompt can cost. Coming back to a 300k-token chat just after its cache expired costs about what 40 prompts would while it's warm.

So a long chat says when that's coming:

- **About to expire:** a clock on the chat's tab and on its workspace's row, with 15 minutes to go. Point at it for how long is left.
- **Expired:** a line above the composer, such as `Cache expired 12m ago · this prompt writes 313.5k tokens again`.

![A chat whose cache is about to expire](../../spec/screens/cache-expiring.png)

What to do about it is yours to judge. If you'll want the chat again, send its next prompt before the clock runs out. If the cache has already expired and the work has moved on, a new chat is often the cheaper way to carry on.

Choosing another model writes the conversation again too, since a cache belongs to one model. The picker says so before you switch.

A chat under 50k tokens says nothing of its cache but in its newest turn's card, since writing that again costs little.

[Context, compacting and the cache](../reference/chats/context-compacting-and-the-cache.md) has every rule.
