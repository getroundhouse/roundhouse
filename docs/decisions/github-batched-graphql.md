# The hub reads pull requests from GitHub in batched GraphQL

**Status:** Decided.
**First recorded:** 2026-11-17, in #1753.

The hub asks GitHub at intervals for what changes only there, such as a pull request's reviews, checks and merges. It asks in GraphQL, and puts every pull request that's due under the same token into as few queries as GitHub's limits on a query allow, across workspaces and repositories. So a hundred workspaces with several pull requests each cost a few queries a round, out of a limit that for most people is 5,000 points an hour.

## Chosen over

- **GitHub's REST API.** Only GraphQL has a pull request's place in the merge queue and its stack.

## What it costs

GraphQL has no conditional requests, which only REST has. So batching is the one thing that keeps a round cheap.
