# GitHub

How GitHub behaves, where more than one place in the code depends on it. The `github` crate makes every call the hub makes to GitHub, and `docs/principles/outside-systems.md` has the rules for calling it.

## The hourly limit on GraphQL

- **GitHub limits each login's GraphQL queries by the hour, in points.** A query costs one point, or more when it asks for lists within lists. For most people the limit is 5,000 points an hour.
- **The limit belongs to the login, not to the program that asks.** The hub shares it with everything else that uses the same login, such as the person's own `gh` and each agent's `gh`.
- **Any GraphQL answer can say what's left.** A query that includes `rateLimit { limit remaining resetAt }` gets back the limit, the points left, and when the points return to the limit. So the hub keeps no count of its own.
- **GitHub counts what one query costs, in points and in time, by how much the query could return.** It gives a query ten seconds. For example, a query for fifty pull requests, each with up to a hundred checks and a hundred entries of its stack, costs a few points and stays well inside the ten seconds.

## What only GraphQL has

- **A pull request's place in the merge queue and its stack are in GraphQL only.** REST has neither.
- **GraphQL has no conditional requests.** Only REST has them. `docs/decisions/github-batched-graphql.md` says what the hub does without them.

## Which pull requests a branch has

- **A branch on GitHub belongs to one repository, under one name, and a pull request is from one branch.** GitHub calls that branch the pull request's head.
- **A pull request's number is unique only in the repository the pull request merges into.** Two repositories can each have a #7, so only a pull request's address tells it apart from every other.
- **A branch lists the pull requests from its own repository and name, whatever repository each merges into.** That's `ref(qualifiedName:) { associatedPullRequests }`, asked of the repository the branch is in. For example, a branch in a fork lists its pull request into the repository the fork was made from.
- **Once a branch is gone, its pull requests are found only by its name.** GitHub can delete a branch when its pull request merges, and a fork can be deleted with its branches. `pullRequests(headRefName:)`, asked of the repository the pull requests merge into, lists them. It also lists the pull requests from every fork's branch of the same name. Each pull request names the repository its head is in, as `headRepository`, which is empty once that repository is deleted.
- **GitHub keeps a branch's pull requests after the branch is merged and deleted, under the branch's name.** A new branch that's given the same name has the old pull requests in its list, along with its own.

## Tokens

- **Renewing a sign-in's token revokes the token it replaces, and the refresh token that renewed it.** GitHub revokes both as it returns the next pair. If the hub loses the new pair, the account has no token that works, and the person has to sign in again. So the hub runs one renewal of an account at a time (`Hub::check_github_logins`), writes the new pair to the account's file before it records when the token ends (`GithubLogins::renewed`), and waits for a renewal that's under way before it stops (`Hub::stop_checking_github_logins`). While an account is being renewed, the hub asks GitHub nothing else with the old token (`GithubLogins::profiles_due`).
- **Making a token for an app's installation revokes nothing.** The installation's earlier tokens work until they expire. If the hub loses a new one, it makes another at its next check, so a hub that stops doesn't wait for one.

## Whether GitHub would take a push

Before `git push` sends anything, it asks GitHub for the repository's refs:

```
GET https://github.com/{owner}/{name}.git/info/refs?service=git-receive-pack
```

It sends the username and token that git's credential helper gives it, as HTTP Basic authentication. GitHub decides at this request whether it takes the push, so its answer covers everything that limits a push to the repository:

- the user's access to the repository
- which repositories a fine-grained token reaches, and its permissions
- which repositories an app's installation was given
- an organization's SAML SSO
- an organization's restrictions on OAuth Apps

The API's `permissions` on a repository gives only the user's role, so it covers the first of these and none of the rest.

A branch's own rules, such as protection, apply only to the push itself. So GitHub can still refuse a push to a protected branch after it has answered 200 here.

| GitHub answers | What it means |
|---|---|
| 200, with the content type `application/x-git-receive-pack-advertisement` | It would take the push |
| 403 | It refuses the push. The body is its reason, which `git` prints after `remote:`, such as `Permission to cli/cli.git denied to ada.` |
| 404 | It has no such repository that the account can see: there's none, or it's private and not the account's |
| 401 | It doesn't take the token |

Any other answer says nothing about the push. The request sends nothing, so it changes nothing.

`Github::push_reach`, in `crates/github/src/reach.rs`, makes this request to check whether the default GitHub account can push to a repository, and the hub records the answer as the repository's `GithubReach`.

## The username of an installation's token

Git gives GitHub an installation's token with the username `x-access-token`, as GitHub's docs say to. GitHub knows whoever pushes with that token as the app's bot, `{slug}[bot]`, and not as `x-access-token`. So for an installation, the username a request was sent with doesn't name the account.

`INSTALLATION_USERNAME`, in `crates/hub/src/github_logins/credential.rs`, is that username. The hub's `DefaultCredential` keeps the bot's name beside it, and a `Reach` names the bot.
