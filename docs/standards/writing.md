# Writing

How to word anything a person reads here: a doc, an issue, a pull request, a commit message, a comment in code, and a message to the owner. `docs/standards/documentation.md` says where each piece of information goes, `docs/standards/issues.md` what an issue's description holds, `docs/standards/pull-requests.md` what a PR's, and `tools/guide/README.md` how the guide's pages are written.

**The reader** is an engineer who knows Roundhouse, its parts and its platforms, and hasn't read the code you're writing about. Often that's the owner, coming from other work.

**The test:** after reading the opening paragraph once, could that reader say what happens, and to what? If they'd have to work out what a phrase refers to, rewrite it. Common words and short sentences don't pass on their own.

## Rules

- **Say what the reader will act on, and stop.** Put first whatever needs their judgment. Leave out how the work got here, and anything that wouldn't change what they do. Give a test and its result rather than an argument. Shorten a text by saying fewer things, not by packing more into each sentence.
- **Start with what happens, in one concrete case:** what a person sees in the app, or what a caller gets back. The code that causes it comes after.
- **Call a thing by its name.** Write "Codex", not "a kind that discovers its models". Code gets its identifier, in backticks. To talk about a group, name it and say who's in it: "agents with a fixed list of models, which are Claude and Antigravity".
- **Use the word the reader already knows.** That's the word `docs/reference/terms.md` gives, such as "hub", and otherwise the standard technical one: a function returns a value, and the hub notifies a client. Don't put an everyday word in its place, such as "answers" for "returns", and don't give a word a special meaning for one text. Quote an identifier as it's spelled, and word the sentence around it by these rules.
- **Introduce a term the app never shows, in a clause,** the first time it matters: "the agent's description, which lists the models and efforts it offers". Don't explain what this reader knows.
- **Give each sentence one idea, and keep its nouns.** Write "changing only the mode", not "that chat's change of mode alone". A longer sentence that reads once is better than a shorter one that reads twice.
- **Follow an abstract statement with an example,** as these rules do. If you can't find one, you may not know what the statement means. Find out, or say what you don't know.
- **Give a title the same care.** It says what goes wrong, or what's true afterwards, in words someone would search for: "Changing a chat's mode fails if its effort is no longer offered".

## Text that's already here

These rules outrank the wording around them, so don't copy the style of a doc, a comment or an earlier PR that breaks one. Rewrite the passage a change touches, and leave the rest.

Read the code a passage is about before rewording it, so its meaning stays the same. If the passage turns out to be wrong, correct it and say so in the PR. If you can't work out what it means, leave it.
