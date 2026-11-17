# Who agents are on GitHub

When an agent pushes, opens a pull request or comments, GitHub records someone as doing it. You choose who on the Git page in [Settings](../reference/settings.md#git): a GitHub account you've added, the bot of an app of your own, or each machine's own login. The account you make the default is used on every machine, for every repository on GitHub.

![The Git page, with three accounts added and the logins found on two machines](../../spec/screens/settings-git.png)

## An account you add

**Sign in with GitHub**, or paste a token, and agents act as you. Every machine's `git` and `gh` use the default for github.com, so agents push and open pull requests as it, and Roundhouse reads pull requests with it, wherever they run. Nothing else on the machine has to be logged in.

- Their pull requests are yours, so GitHub won't let you approve them, and nothing on GitHub tells their work from yours.
- What they ask of GitHub counts against your rate limit.

Signing in shows a code, which you type on GitHub and approve there. Agents can then do what `gh` logged in as you could: reach every repository you can, in every organization that lets Roundhouse in. An organization that limits which apps can reach it has to approve Roundhouse first. The hub renews the login by itself, so it keeps working until you disconnect it, unless GitHub won't renew it, such as after about six months with the hub stopped. Then it needs signing in again.

A token does the same. Use one when an organization hasn't approved Roundhouse but allows tokens, or to keep agents to a few repositories: a fine-grained token reaches only the repositories you choose, of one owner. It ends when you said it would, and you renew it yourself, with a new one, from its row's `…` menu.

The hub keeps either in its own data, as [Where data is kept](../reference/where-data-is-kept.md) says, and hands the default's to every machine, where agents' `git` and `gh` use it. The app writes it nowhere.

## An app of your own

Make a GitHub App of your own, and agents act as its bot, such as `ada-roundhouse[bot]`, rather than as you: they push, open pull requests, comment and commit as it.

- Their pull requests are the bot's, so you can review and approve them, and GitHub tells their work from yours.
- What they ask of GitHub counts against the app's own rate limit, not yours.
- They reach only the repositories you install it on, with what the app may do: push, open and merge pull requests, open issues and comment, change workflows, and read runs and checks. You can give it more on GitHub, on the app's page.

The app is private, so it's made under the account or organization whose repositories it's for, and only that one can install it. Make one under each organization agents work in. Only an organization's owners and app managers can make one under it.

Making one takes three steps in the panel, as [Settings](../reference/settings.md#make-an-app-of-your-own) says: choose where to make it and its name, make it on GitHub's page, which your browser opens, then install it on the repositories agents should reach. Only the hub holds the app's key, and it makes the installation a token from it an hour at a time, so there's nothing to renew. Each installation is a row on the Git page, which you make the default as you would an account.

![Git, with an app's installation as the default](../../spec/screens/settings-git-app.png)

Removing the app, from its row's `…` menu, has the hub forget its key, and deletes nothing on GitHub. Delete it there, in the developer settings of your account or the organization, to be rid of it.

## Each machine's own login

Until you add an account, and whenever you make it the default, agents use the login each machine already has, as they would without Roundhouse, and only on that machine. Roundhouse never copies it. Under **Found**, the Git page says who `gh` is logged in as on each machine where a workspace has run since the hub last started, and whether GitHub took its token then. `git` can push with something else, such as an SSH key, which Roundhouse doesn't see.

A repository that isn't on GitHub, such as one on GitLab or GitHub Enterprise, is always reached with the machine's own login, and its commits are by whoever its own Git config or the machine's says. On a machine whose Git is older than 2.36, so are the commits in every repository, though agents there still push to GitHub as the default.

## When the default can't be used

If the default's token ends, you disconnect it, it needs signing in again, or it's an app's installation that was uninstalled on GitHub, it stays the default, and agents can't push until you renew it, sign in again, or make another the default. They don't fall back on the machine's own login, so nothing is pushed as someone you didn't choose. The Git page says what's wrong, and what to do, at the top of **Added**.

A signed-in account's token lasts 8 hours, and the hub renews it before it ends, so this happens only if the hub couldn't reach GitHub in time. Git says it's `Renewing` until the hub has. An app's installation's token lasts an hour, and the hub makes the next well before it ends, so the same goes for it: while GitHub doesn't make the next, a default installation says `Renewing` once the last has ended.

## When the default can't push to a repository

The hub asks GitHub whether the default can push to the repository on GitHub that each repository's `origin` names, as `git push` would ask, so it meets what a push would: your access, a fine-grained token's repositories, the repositories an app of your own is installed on, and an organization that hasn't approved Roundhouse. It asks as it starts, whenever the default or its login changes, and every half hour, since what an account can do changes on GitHub too.

If GitHub refuses, has no such repository the default can see, or doesn't take its token, every workspace of that repository says so in a line above the composer, with GitHub's words and what to do. **Open Git** opens the Git page, where you can make another account the default or add one that can push there. It blocks nothing: agents push as they would anyway. The line goes once the hub finds a push would be taken.

![A line above the composer saying GitHub refuses pushes as the default](../../spec/screens/push-refused.png)

When the default is an app of your own, the line names its bot. For a repository of the account or organization the app is installed on, it says to install the app there with access to the repository, and **Install on GitHub** opens the app's page on GitHub, where you give it more repositories. A repository of anyone else's the app can't reach, since it's installed only where it was made, so the line says to make another the default, such as an app made under that owner. If GitHub doesn't take the app's token, as after it was uninstalled, the line says to install it again and connect it again.

![The line for an app of your own, with Install on GitHub](../../spec/screens/push-refused-app.png)

While each machine's own login is the default, the hub doesn't ask, and no workspace says.

## More than one

You can add more than one account, but only the default is used.

## Which to choose

- **Your own repositories:** sign in.
- **Repositories you don't own, such as open source:** sign in.
- **A few repositories, or an organization that hasn't approved Roundhouse:** a fine-grained token.
- **Agents' work told apart from yours, in pull requests you can approve:** an app of your own.
- **An organization's repositories, as a bot of the organization's own:** an app of your own, made under it.
- **Whatever each machine is already logged in as:** each machine's own login.
