# Observations

These rules cover observations, the hub's copies of what others own: what one is, where it comes from, where the hub keeps it, and how a client keeps its copy. `docs/principles/serving.md` says what else a hub serves and what the hub keeps current, and `docs/principles/outside-systems.md` how the hub reads GitHub.

## What an observation is

- **An observation is the hub's copy of state someone else owns, as the hub last observed it.** Git, GitHub and an agent each own state the hub only observes: by watching it, by asking for it, or by being told as it changes. Where the hub can ask again, it does, to keep its copy current. Where it can't, as with what only a running agent says, its copy is as of when that was last said. What an agent says in an event is the log's, so what the hub works out from it is a record, such as a session's status and what its running agent claims. *Not built:* such a copy is a record or a field of one, as are a workspace's branches, whether the default GitHub account can push to a repository, an agent's list of models, an account's usage limits, how full an agent's context is, an agent's todos, and how many files a workspace has uncommitted. Or it isn't kept, as a workspace's diff is read whole where its hub keeps no current summary of its changed files.
- **What the hub reads from an outside system is a cache, and what Roundhouse decides about it is the hub's own record, which keeps only the link.** Which pull request is a workspace's, and which account a project uses, are Roundhouse's decisions, so they're records and the truth. A workspace's record keeps its pull request's address, and what GitHub says of the pull request is in the cache. What's read from GitHub can be read again, so losing it loses nothing. A record can't always be rebuilt: once a workspace's branch is deleted, or its name is used again, GitHub alone can't say which pull request the workspace had.
- **An observation says how fresh it is: not known, current, or as of a time.** It's current while something keeps it up to date, and otherwise as it was last read, with when. So no client shows an old reading as current, or takes "not known" for "none". Each read replaces it whole, and its revision moves only when what it says changes, so reading the same again sends clients nothing.

  ```rust
  // crates/protocol/src/model/observation/observed.rs, abridged
  pub enum Observed<T> {
      /// Never read, or emptied since.
      NotKnown,
      /// Kept up to date now, as what a client follows is.
      Current { value: T },
      /// As it was last read, at `as_of_ms`, since nothing keeps it up to date now.
      AsOf { value: T, as_of_ms: i64 },
  }
  ```
- **An observation holds the whole of what it's of, unless that's too big for every client that follows it to hold.** Only then is it split: the observation holds a summary, and the detail is read on request from whoever owns it, a worker or GitHub, in pages past a size, and the hub keeps none of it. The summary names a version of each piece of detail, so a client showing the detail reads it again only when its version changes. For example, an agent's list of models is an observation whole. A workspace's changed files, each with its lines added and removed, are a summary, and one file's diff is read on request. *Not built:* an agent's list of models as an observation.
- **A workspace's summary of its changes names at most 5,000 files, those git tracks first and then the untracked ones, each in order of their paths, and says how many it leaves out, with its totals over all of them.** So an unignored folder of 300,000 files neither sends every client that follows the workspace 45 MB on each change, nor crowds out the agent's ten edits. A client lists none of the files past the 5,000, and says how many there are.
- **A value the hub works out from other state is an observation if any of that state is one,** and the hub works it out again when any of it changes. For example, how a workspace's pull request stands is worked out from the workspace's record and from what GitHub says, so it's an observation.

## Where it comes from, and where it's kept

- **What a worker watches reaches the hub only from the workspace's current worker, and each report replaces the one before it whole.** A report from a worker that's been replaced is dropped, and nothing is merged. For example, a worker sends every branch of its worktree each time one moves. *Unconfirmed.*
- **The cache is a file of its own, apart from the hub's database, which the hub empties rather than migrates.** It has a version of its own. When the version isn't the hub's, as after an update that changes the cache's form, the hub empties it and reads GitHub again, so the cache needs no migration and no copy before one.
- **Every observation is kept in that cache, and in memory, each kind under a version of its own.** A hub that starts again has each as of when it was read, and a change to one kind's form empties only that kind. What the hub can't ask for again is then not known until its owner next says it, and for good if it never does, as with the email of an account whose login has gone from every machine. *Not built:* a workspace's lines added and removed, its changed files, and what GitHub says of a chip's issue or pull request and of a workspace's own, are the only observations the hub keeps in it.
- **What's read from GitHub is kept by what it describes, and each observation of it is worked out from that, kept as a kind of its own, and gone once what it's worked out from is forgotten.** So a pull request that's a workspace's own and is named in a chip is read once, into GitHub's part of the cache, under a version of its own, while each level it's sent at has an observation of it. For example, a chip's observation is worked out from the summary GitHub's part keeps, and goes once that part forgets the summary, as it does what nothing has asked for in a week. *Unconfirmed.*

## Keeping a copy

- **A client keeps an observation as it keeps a record: by revision, whichever level sent it.** So `client-core` holds records and observations one way, as `docs/principles/records.md` says of a record.
- **A client holds each thing in the one form its hub sends it in, and reads it from that.** A hub from before an observation sends what it's of in the form from before it, so taking in one form of something lets go of the other, which only a connection to another hub can have left. For example, a workspace's pull request is read from `GithubUpdate` where the hub sends it so, and from its observations where the hub sends those. *Unconfirmed.*
