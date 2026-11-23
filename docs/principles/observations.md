# Observations

These rules cover observations, the hub's copies of what others own: what one is, where it comes from, where the hub keeps it, and how a client keeps its copy. `docs/principles/serving.md` says what else a hub serves and what the hub keeps current, and `docs/principles/outside-systems.md` how the hub reads GitHub.

## What an observation is

- **An observation is the hub's copy of state someone else owns, as the hub last observed it.** Git, GitHub and an agent each own state the hub only observes: by watching it, by asking for it, or by being told as it changes. Where the hub can ask again, it does, to keep its copy current. Where it can't, as with what only a running agent says, its copy is as of when that was last said. What an agent says in an event is the log's, so what the hub works out from it is a record, such as a session's status and what its running agent claims. *Not built:* such a copy is a record or a field of one, as are a workspace's branches, whether the default GitHub account can push to a repository, an agent's list of models, an account's usage limits, how full an agent's context is, an agent's todos, and how many files a workspace has uncommitted. Or it's in GitHub's cache, which reaches a client with no revision. Or it isn't kept, as a workspace's diff is read whole on each request.
- **What the hub reads from an outside system is a cache, and what Roundhouse decides about it is the hub's own record, which keeps only the link.** Which pull request is a workspace's, and which account a project uses, are Roundhouse's decisions, so they're records and the truth. A workspace's record keeps its pull request's address, and what GitHub says of the pull request is in the cache. What's read from GitHub can be read again, so losing it loses nothing. A record can't always be rebuilt: once a workspace's branch is deleted, or its name is used again, GitHub alone can't say which pull request the workspace had.
- **An observation says how fresh it is: not known, current, or as of a time.** It's current while something keeps it up to date, and otherwise as it was last read, with when. So no client shows an old reading as current, or takes "not known" for "none". Each read replaces it whole, and its revision moves only when what it says changes, so reading the same again sends clients nothing. *Not built.*

  ```rust
  // Not built. A sketch of what the protocol would hold.
  pub enum Observed<T> {
      /// Never read, or emptied since.
      NotKnown,
      /// Kept up to date now, as what a client follows is.
      Current(T),
      /// As it was last read, at `as_of_ms`, since nothing keeps it up to date now.
      AsOf { value: T, as_of_ms: i64 },
  }
  ```
- **An observation holds the whole of what it's of, unless that's too big for every client that follows it to hold.** Only then is it split: the observation holds a summary, and the detail is read on request from whoever owns it, a worker or GitHub, in pages past a size, and the hub keeps none of it. The summary names a version of each piece of detail, so a client showing the detail reads it again only when its version changes. For example, an agent's list of models is an observation whole. A workspace's changed files, each with its lines added and removed, are a summary, and one file's diff is read on request. *Not built.*
- **A value the hub works out from other state is an observation if any of that state is one,** and the hub works it out again when any of it changes. For example, how a workspace's pull request stands is worked out from the workspace's record and from what GitHub says, so it's an observation. *Not built.*

## Where it comes from, and where it's kept

- **What a worker watches reaches the hub only from the workspace's current worker, and each report replaces the one before it whole.** A report from a worker that's been replaced is dropped, and nothing is merged. For example, a worker sends every branch of its worktree each time one moves. *Unconfirmed.*
- **The cache is a file of its own, apart from the hub's database, which the hub empties rather than migrates.** It has a version of its own. When the version isn't the hub's, as after an update that changes the cache's form, the hub empties it and reads GitHub again, so the cache needs no migration and no copy before one.
- **Every observation is kept in that cache, and in memory, each kind under a version of its own.** A hub that starts again has each as of when it was read, and a change to one kind's form empties only that kind. What the hub can't ask for again is then not known until its owner next says it, and for good if it never does, as with the email of an account whose login has gone from every machine. *Not built:* only what's read from GitHub is kept this way, under one version.

## Keeping a copy

- **A client keeps an observation as it keeps a record: by revision, whichever level sent it.** So `client-core` holds records and observations one way, as `docs/principles/records.md` says of a record. *Not built.*
