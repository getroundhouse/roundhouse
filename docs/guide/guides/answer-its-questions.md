# Answer its questions

An agent asks you something when it needs a decision only you can make, when it's in plan mode and wants to do something, and before the few things its own CLI asks about whatever the mode. Each time, its workspace's title turns bright until you've answered.

## A question

A question takes the composer's place, so you answer where you'd have typed.

![A question from the agent, with choices](../../spec/screens/question.png)

- **Press a number** to choose that option.
- **Press `0`** to type an answer of your own, when the question allows it.
- **Press `Enter`** to go on to the next question without an answer, and to send once there's none.

When the agent asks several at once, a dot for each shows which have an answer, and the left and right arrow keys move between them. You don't have to answer them all: the send arrow works once any has an answer, and leaves the others out.

![Three questions, with the first answered](../../spec/screens/questions.png)

The `×` skips them all, and tells the agent to carry on if it can.

What you were typing in the composer isn't lost. It waits behind the question, and comes back when the question goes.

## An approval

In plan mode, an agent asks before a tool needs permission, such as a command that changes files. The request takes the composer's place, as a question does, with the command itself under what the agent says it's for.

![A command waiting to be allowed, in a chat that's planning](../../spec/screens/approval.png)

- **Press `1` to allow it, or `2` to deny it,** then `Enter` to send.
- **Press `0` to say what to do instead, when offered.** What you type denies the request, and the agent is told in your words.

An agent with full access asks only about what its own CLI won't do unasked, such as a command that removes something it can't check. The request says why, and which subagent is asking, when one is. **Skip Claude's safety checks**, in Settings, lets Claude's requests run without asking. Codex offers Allow and Deny for a tool's use; it does not offer a written reason for denial.

![A removal waiting to be allowed, asked about with full access](../../spec/screens/approval-full-access.png)

When several agents ask at once, each is asked in turn, and the bottom left says how many are waiting.

When the agent has worked out its plan, it asks you to approve it, on a card in the chat. Approving goes back to full access, and the agent carries the plan out.

[Full access and plan mode](../concepts/full-access-and-plan-mode.md) says why agents otherwise don't ask.

## While it waits

An agent waiting on you holds still, and so does its mark. Nothing else is held up: other chats and workspaces carry on.

An ordinary answer you've started is kept by the hub, so it's there if you quit the app and come back, and wherever else the chat is open. A secret answer is masked, stays only in this window until submitted, and is omitted from the chat log.

## Afterwards

Once answered, a question is a line in the chat where the agent asked it, saying `Asked` and what you answered. Click it to see the whole question again.

[Questions and approvals](../reference/chats/questions-and-approvals.md) has every key.
