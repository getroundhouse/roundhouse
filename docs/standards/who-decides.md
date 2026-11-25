# Who decides

Which decisions are the owner's and which are the agents', how an owner doc changes, and how an agent proposes a change to one. `docs/README.md` says which docs are the owner's.

The owner decides what the product is: what it does, how it looks, how fast it is and how it should feel. The owner also decides its high-level architecture and the big technical choices, such as the UI framework. The architecture is the parts and how they fit together: who owns each piece of state, the interfaces between parts, what's stored or sent, and how a failure in one part is kept from the others. And the owner decides the standards everything is built to, including the files that enforce them: the lints in `Cargo.toml` and `clippy.toml`, `rustfmt.toml`, `rust-toolchain.toml`, and `tools/policy`. Agents decide everything else, and keep the reference docs, which record facts they find.

An owner doc, or a file that enforces a standard, changes only with the owner's approval, given in the PR or in conversation. The spec, the principles and the standards take one kind of change without it. When work needs a choice they don't make, an agent makes the smallest reasonable one and adds it marked *Unconfirmed*, in the same PR, unless it's a choice of architecture, which it asks the owner about first, as `AGENTS.md` says, or of how something looks, which is put on the design's board to decide, as `docs/standards/design.md` says. Confirming a rule, and changing or removing a confirmed one, are the owner's.

A choice reaches an owner doc only when it belongs in one, as "Where a sentence goes" in `docs/standards/documentation.md` says. A choice that a person using the product wouldn't notice, and that no principle settles, is the agents' to make. Its reason goes in a comment beside the code, and no doc records it.

Agents push and merge as the owner's GitHub account, so GitHub can't tell them apart. This holds by convention, and nothing enforces it.

Two marks show where an owner doc and the product stand apart. *Not built* ends a rule that's decided and not in the product yet. *Unconfirmed* ends one an agent chose that the owner hasn't confirmed.

What the owner has decided isn't fixed. An agent that sees a better way, in the product, the architecture or a standard, says so rather than quietly working around it or going along with it. A small change goes in a PR that changes the doc, with the reasoning, and waits for the owner. A larger one is an issue labelled `kind/question` and `needs/owner`, saying what's decided, what would be better, the evidence, what switching costs, and a recommendation. Until the owner agrees, the work follows what's decided. Every rule gives its reason, so it can be challenged, and a proposal the owner turned down comes back only with something new.

Four searches cover everything waiting on the owner:

| Queue | What's in it | Search |
|---|---|---|
| Decisions | Questions only the owner can answer | [Issues with `needs/owner` and `kind/question`](https://github.com/getroundhouse/roundhouse/issues?q=is%3Aissue+is%3Aopen+label%3Aneeds%2Fowner+label%3Akind%2Fquestion) |
| Tasks | What only the owner can do, such as trying something by hand | [Issues with `needs/owner` that aren't questions](https://github.com/getroundhouse/roundhouse/issues?q=is%3Aissue+is%3Aopen+label%3Aneeds%2Fowner+-label%3Akind%2Fquestion) |
| Unconfirmed | Agents' choices in owner docs, waiting to be confirmed | [*Unconfirmed* in `docs/`](https://github.com/search?q=repo%3Agetroundhouse%2Froundhouse+path%3Adocs+Unconfirmed&type=code). It also finds this doc and the spec's README, which define the mark |
| Approvals | PRs that change an owner doc or a standard | [PRs with `needs/owner`](https://github.com/getroundhouse/roundhouse/pulls?q=is%3Apr+is%3Aopen+label%3Aneeds%2Fowner) |

From a terminal, `gh issue list -l needs/owner` shows the first two, `git grep Unconfirmed docs` the third, and `gh pr list -l needs/owner` the last.
