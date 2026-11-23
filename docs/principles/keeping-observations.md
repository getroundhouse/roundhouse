# Keeping observations

These rules cover how an observation, the hub's copy of what others own, reaches the hub, where the hub keeps it, and how a client keeps its copy. `docs/principles/observations.md` says what an observation is, what it holds and how fresh it says it is. It also says that what the hub reads from an outside system is a cache, the one these rules keep observations in. `docs/principles/outside-systems.md` says how the hub reads GitHub.

## Where it comes from

- **What a worker watches reaches the hub only from the workspace's current worker, and each report replaces the one before it whole.** A report from a worker that's been replaced is dropped, and nothing is merged. For example, a worker sends every branch of its worktree each time one moves. *Unconfirmed.*
- **What an agent says only part of at a time is merged with what's kept, rather than replacing it, each part as it was last heard, with when.** So a part heard earlier, as one a worker sends again to a hub that connects, never replaces one heard later, and a hearing that says the same again of each part tells no client. For example, an agent says some of its account's usage limits at a time, and each window is kept as it was last said. *Unconfirmed.*

## Where the hub keeps it

- **The cache is a file of its own, apart from the hub's database, which the hub empties rather than migrates.** It has a version of its own. When the version isn't the hub's, as after an update that changes the cache's form, the hub empties it and reads GitHub again, so the cache needs no migration and no copy before one.
- **Every observation is kept in that cache, and in memory, each kind under a version of its own, but what doesn't outlast the hub, such as which build a worker is, which is kept in memory alone.** A hub that starts again has each as of when it was read, and a change to one kind's form empties only that kind. What the hub can't ask for again is then not known until its owner next says it, and for good if it never does, as with the email of an account whose login has gone from every machine. *Not built:* a workspace's lines added and removed, its changed files, what GitHub says of a chip's issue or pull request and of a workspace's own, what an agent offers where a chat would launch, who an agent's account is and an account's usage limits, are the only observations the hub keeps in it, and none is kept in memory alone.
- **What's read from GitHub is kept by what it describes, and each observation of it is worked out from that, kept as a kind of its own, and gone once what it's worked out from is forgotten.** So a pull request that's a workspace's own and is named in a chip is read once, into GitHub's part of the cache, under a version of its own, while each level it's sent at has an observation of it. For example, a chip's observation is worked out from the summary GitHub's part keeps, and goes once that part forgets the summary, as it does what nothing has asked for in a week.

## Keeping a copy

- **A client keeps an observation as it keeps a record: by revision, whichever level sent it.** So `client-core` holds records and observations one way, as `docs/principles/records.md` says of a record.
- **A client holds each thing in the one form its hub sends it in, and reads it from that.** A hub from before an observation sends what it's of in the form from before it, so taking in one form of something lets go of the other, which only a connection to another hub can have left. For example, a workspace's pull request is read from `GithubUpdate` where the hub sends it so, and from its observations where the hub sends those.
