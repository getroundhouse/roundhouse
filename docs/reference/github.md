# GitHub

How GitHub and `gh`, its command line, behave, where more than one place in the code depends on it. The `github` crate makes every call the hub makes to GitHub, and `docs/principles/outside-systems.md` has the rules for calling it.

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

## Signing in through the device flow

- **The device flow takes an OAuth App's client ID and no client secret.** That holds for asking for a code, asking after its token and renewing the token, as a real renewal showed in November 2026. So the client ID is no secret, and every hub sends the same one, `ROUNDHOUSE_APP` in `crates/github/src/config.rs`.
- **A device code and the client ID are all it takes to get a sign-in's token.** So the hub keeps the device code only in the memory of the job that asks after the sign-in (`Hub::wait_for_sign_in`), and a sign-in that's waiting ends when its hub stops.
- **A code lasts 15 minutes, and GitHub says how often to ask after its token.**
- **GitHub can answer an ask after the token with an error that asking again won't change,** such as `device_flow_disabled`, for an app that doesn't let people sign in through the device flow. That's neither the person refusing nor the code ending, so a sign-in's record has a state of its own for it, `GithubSignInStatus::Failed`.
- **A sign-in's token lasts 8 hours, and its refresh token about 181 days.** GitHub says how long each lasts as it gives them. "Tokens" says what renewing one does.

## Apps and their installations

- **A private GitHub App can be installed only on the account or organization it was made under.** So it has at most one installation, on its owner. GitHub lists more only for an app that was made public later. `docs/decisions/private-app-of-your-own.md` says why an app of your own is private.
- **GitHub's manifest flow sends the person back with a code, which GitHub takes once, within an hour.** `POST /app-manifests/{code}/conversions` turns the code into the app's number, slug, owner and private key. GitHub returns a client secret and a webhook secret too, which the hub doesn't read (`Github::make_app`).
- **An app's bot is a user of its own, `{slug}[bot]`, with its own number.** GitHub links a commit to the bot by the address `<number>+<slug>[bot]@users.noreply.github.com`, so the hub asks GitHub for the bot by name to learn its number (`Github::bot`).
- **An installation's token lasts an hour.** "Tokens" says what making the next one does.
- **Once an app is uninstalled from an account, GitHub stops listing that installation, and answers 404 when asked for its token.** The hub takes either to mean the installation is gone (`GithubLogins::installations_found`, `Minted::Gone`).
- **An installation's calls count against an hourly limit of its own,** not the limit of the person who made the app.

## `gh`'s own login

A worker reads its machine's own `gh` login for the hub, as `FindGithubLogin` says, and these decide how.

- **`gh` uses a `GH_TOKEN` or `GITHUB_TOKEN` from its environment over its own login.** Every process a worker starts under the machine's own login inherits the worker's environment, so such a token is who those processes push as.
- **`gh auth status` names the login from `gh`'s own config, even when GitHub can't be reached.** `gh` 2.81 is the first to print it as JSON.
- **`gh` 2.103 renews a token that it renews by itself whenever it reads the token.** Renewing rotates the refresh token, and `gh` forgets the login if GitHub refuses it. So reading the login could change it. `--no-refresh`, on `gh auth status` and `gh auth token`, reads without renewing.
- **GitHub answers 401 for such a token once its time is up, until `gh` next runs and renews it.** So that 401 doesn't mean `gh` needs logging in again (`Github::found_login_works`).
