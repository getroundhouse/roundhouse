# Who agents are on GitHub

When an agent pushes, opens a pull request or comments, GitHub records someone as doing it. You choose who on the Git page in [Settings](../reference/settings.md#git): a GitHub account you've added, or each machine's own login. The account you make the default is used on every machine, for every repository on GitHub.

![The Git page, with three accounts added and the logins found on two machines](../../spec/screens/settings-git.png)

## An account you add

**Sign in with GitHub**, or paste a token, and agents act as you. Every machine's `git` and `gh` use the default for github.com, so agents push and open pull requests as it, and Roundhouse reads pull requests with it, wherever they run. Nothing else on the machine has to be logged in.

- Their pull requests are yours, so GitHub won't let you approve them, and nothing on GitHub tells their work from yours.
- What they ask of GitHub counts against your rate limit.

Signing in shows a code, which you type on GitHub and approve there. Agents can then do what `gh` logged in as you could: reach every repository you can, in every organization that lets Roundhouse in. An organization that limits which apps can reach it has to approve Roundhouse first. The hub renews the login by itself, so it keeps working until you disconnect it, unless GitHub won't renew it, such as after about six months with the hub stopped. Then it needs signing in again.

A token does the same. Use one when an organization hasn't approved Roundhouse but allows tokens, or to keep agents to a few repositories: a fine-grained token reaches only the repositories you choose, of one owner. It ends when you said it would, and you renew it yourself, with a new one, from its row's `…` menu.

The hub keeps either in its own data, as [Where data is kept](../reference/where-data-is-kept.md) says, and hands the default's to every machine, where agents' `git` and `gh` use it. The app writes it nowhere.

## Each machine's own login

Until you add an account, and whenever you make it the default, agents use the login each machine already has, as they would without Roundhouse, and only on that machine. Roundhouse never copies it. Under **Found**, the Git page says who `gh` is logged in as on each machine where a workspace has run since the hub last started, and whether GitHub took its token then. `git` can push with something else, such as an SSH key, which Roundhouse doesn't see.

A repository that isn't on GitHub, such as one on GitLab, is always reached with the machine's own login.

## When the default can't be used

If the default's token ends, you disconnect it, or it needs signing in again, it stays the default, and agents can't push until you renew it, sign in again, or make another the default. They don't fall back on the machine's own login, so nothing is pushed as someone you didn't choose. The Git page says what's wrong, and what to do, at the top of **Added**.

A signed-in account's token lasts 8 hours, and the hub renews it before it ends, so this happens only if the hub couldn't reach GitHub in time. Git says it's `Renewing` until the hub has.

## When the default can't push to a repository

The hub asks GitHub whether the default can push to the repository each repository's `origin` pushes to, as `git push` would ask, so it meets what a push would: your access, a fine-grained token's repositories, and an organization that hasn't approved Roundhouse. It asks as it starts, whenever the default or its login changes, and every half hour, since what an account can do changes on GitHub too.

If GitHub refuses, has no such repository the default can see, or doesn't take its token, every workspace of that repository says so in a line above the composer, with GitHub's words and what to do. **Open Git** opens the Git page, where you can make another account the default or add one that can push there. It blocks nothing: agents push as they would anyway. The line goes once the hub finds a push would be taken.

![A line above the composer saying GitHub refuses pushes as the default](../../spec/screens/push-refused.png)

While each machine's own login is the default, the hub doesn't ask, and no workspace says.

## More than one

You can add more than one account, but only the default is used.

## Which to choose

- **Your own repositories:** sign in.
- **Repositories you don't own, such as open source:** sign in.
- **A few repositories, or an organization that hasn't approved Roundhouse:** a fine-grained token.
- **Whatever each machine is already logged in as:** each machine's own login.
