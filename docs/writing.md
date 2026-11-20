# Writing

How to word anything a person reads here: a doc, an issue, a pull request, a commit message, a comment in code, and a message to the owner. `docs/documentation.md` says where each piece of information goes, and `docs/pull-requests.md` says what a PR's description holds. `tools/guide/README.md` covers the guide's pages, which are written for someone using the app.

**The reader** is an engineer who knows Roundhouse as a product and hasn't read the code you're writing about. Often that's the owner, coming from other work.

**The test:** after reading the first paragraph once, could that reader say what happens, and to what? If they'd have to stop and work out what a phrase refers to, rewrite it.

Common words and short sentences don't pass this test on their own. "A chat whose record holds settings its kind no longer takes can't have its mode changed by itself" has no rare word in it, and it takes three reads. The rules below cover the ways text most often fails the test.

## Rules

- **Start with what happens, in one concrete case.** Say what a person sees in the app, or what a caller gets back. The code that causes it comes after. Write "If a chat's saved effort is no longer offered, changing just its approval mode fails, with an error about the effort", not "`LiveSession::configure` refuses a change unless the kind takes the settings the change leaves the chat with".
- **Call a thing by its name.** If you mean Codex, write "Codex", not "a kind that discovers its models". A function, type, file or setting that has a name gets it, in backticks when it's code. When a group matters more than its members, name the group and then say who's in it: "agents with a fixed list of models, which are Claude and Antigravity".
- **Use the standard technical word when there is one.** A function returns a value, an agent supports an effort, and the hub notifies a client. Write those, not "answers", "takes" or "tells of". The reader already knows the technical word, and can't tell that an everyday one is standing in for it. A plain word is the one the reader would use, which isn't always the most common one.
- **Use a project term only as `docs/terms.md` defines it, and don't invent one for a single text.** A word such as "fault", given a special meaning for one issue, reads as ordinary English and misleads. If an idea needs a word of its own, add it to `docs/terms.md`. Otherwise spell the idea out each time.
- **Introduce a term the reader may not know.** The first time a term the app never shows carries a sentence, say what it is, as in "the agent's description, which lists the models and efforts it offers".
- **Give each sentence one idea, and keep its nouns.** A sentence with three "which" clauses is three sentences. Don't shorten a sentence by leaving out what it's about: write "the icons are now embedded by `look::Assets`", not "the icons are now `look`'s", and "changing only the mode", not "that chat's change of mode alone". A longer sentence that reads once is better than a shorter one that reads twice.
- **Follow an abstract statement with an example.** "A setting that depends on another, such as an effort on its model" lets the reader check they understood. Without the example, they can't.
- **Give a title the same care.** It says what goes wrong, or what's true afterwards, in words someone would search for: "Changing a chat's mode fails if its effort is no longer offered", not "A change is refused for a fault the chat already had".

## A bug's issue

An issue for a bug has these parts, in this order:

1. What goes wrong and when, in a sentence or two that someone who hit it would recognize.
2. How a person gets there, when it isn't obvious.
3. Why it happens, naming the code.
4. Ways to fix it, when you see any.

It says which of this was seen in the app and which was only read from the code.

## Text that's already here

These rules outrank the wording around them. Where a doc, a comment or an earlier PR breaks one, don't copy its style. Rewrite the passage a change touches, and leave the rest for a change of its own.
