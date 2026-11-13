# Questions and approvals

What an agent can ask you, and how each is answered.

## Questions

A question from the agent takes the composer's place until it's answered. What's typed and attached in the composer waits behind it.

![A question from the agent, with choices](../../../spec/screens/question.png)

It shows one question at a time: its header and text, then each option on a numbered row with what it means under it, and a last row, `0`, to type another answer into.

### Keys

| Key | What it does |
|---|---|
| A number | Chooses that option |
| `0` | Goes to the row for typing an answer |
| Left and right arrows | Move between questions, when there are several |
| `Enter` | Goes to the next question without an answer, and sends once there's none |
| Escape | Leaves the typing row |

The question takes the keys when it comes. When it's just come, keys do nothing until half a second passes without one, so the end of something you were typing can't answer it by accident. Choosing with the pointer works at once.

### Several questions

With more than one, a dot for each sits at the bottom left, filled once it has an answer. Where a question takes several answers, a check marks each one chosen.

![Three questions, with the first answered](../../../spec/screens/questions.png)

- **The arrow sends once any question has an answer,** and leaves out the ones without.
- **The `×` skips them all,** and tells the agent to carry on if it can.
- **Questions waiting are asked in turn,** oldest first.

### Once answered

An answered question is a line in the chat where the agent asked it: `Asked`, each question's header, and what was answered. Click it for the whole question. One nobody answered says `Not answered`.

![Answered questions, opened](../../../spec/screens/question-answered-opened.png)

## Approvals

Only an agent in plan mode asks before using a tool. The request is a card in the chat, in amber, with `Allow` and `Deny`.

![A command waiting to be allowed, in a chat that's planning](../../../spec/screens/approval.png)

When its plan is ready, the agent asks you to approve it. Approving goes back to full access.

## While an agent waits

- **Its title is bright,** in its chat's tab and on its workspace's row, until it's answered, whether or not it's showing.
- **Its mark holds still,** and the working line goes, since the question says it's your turn.
- **A notification says `Waiting on you`,** if you're elsewhere.
- **What you've chosen and typed in answer is kept by the hub,** so it's there when the app opens again.

## From the command line

`roundhouse approve` and `roundhouse answer` do the same from a terminal. See [The command line](../command-line.md).
