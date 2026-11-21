# The iPhone app

One hub's workspaces and chats, on a phone, for keeping work moving away from the desk. It does less than the desktop app on purpose: it shows what the agents are doing, answers them, and starts new work, and leaves reviewing and merging to the desktop.

## Reaching the hub

- **A phone pairs with one hub, by scanning a QR code with the Camera app.** `roundhouse phone pair`, on the hub's computer, shows the code and waits. The code opens the app with a link, and the app pairs without anything to type. The link can be pasted too, for a phone that can't scan. Pairing with another hub replaces the first, so a link opened while paired asks first, since any app can open one. *Unconfirmed.*
- **The phone reaches the hub directly,** over the same network or a VPN to it, such as Tailscale, and only while the hub's computer is awake. The hub's QR code holds every address the phone might reach it at, and the phone tries them all, each a moment after the last, and uses whichever answers first, so an address it can't reach doesn't hold it up. *Not built:* a relay, for a hub that a phone can't reach directly. *Unconfirmed.*
- **What the phone keeps is enough to connect again, and no more.** The token the hub gave it is in the Keychain, for this phone only, so no backup carries it to another, and the hub's name, addresses and key are in the app's own files. Removing the phone on the hub, with `roundhouse phone revoke`, closes its connections and stops the token working. *Unconfirmed.*
- **A lost connection comes back without asking.** The app tries again after a second, then waits twice as long after each failure, up to 30 seconds, and says it's trying. Coming back to the app after 20 seconds away connects again at once, since a phone that slept may hold a connection that's gone without a word. What's typed and what's shown stay as they were. *Unconfirmed.*
- **The hub screen says how the connection stands and where the phone looks for the hub,** with a reminder that the phone needs the hub's network or a VPN to it, and a button to forget the hub and pair again. A hub the phone couldn't forget, such as when the Keychain refuses, stays, with why, since the app would follow it again the next time it starts. Forgetting leaves the token working on the hub until it's revoked there. *Unconfirmed.*

## Workspaces

- **The first screen lists the hub's workspaces, grouped by repository,** newest first, each with the status mark the desktop's sidebar shows, in its pull request's colour as the hub sends it, what the sidebar calls it, which is its title or its name until it has one, and its branch where that isn't the same again. Only workspaces that are ready are listed, since one still being made or archived has nothing to open. *Unconfirmed.*
- **Tapping a workspace opens its chat:** the one last worked in, as the desktop opens it on, or with none open, the one the hub finds or starts, on the agent the hub would start a chat on. *Unconfirmed.*
- **`+` starts a workspace in a repository the hub already has,** with the hub's choice of name and branch, as the desktop app does, and opens its chat. Adding a repository needs the hub's computer. *Unconfirmed.*

## A chat

- **A chat shows the same transcript as the desktop,** live as the agent works, with what the workspace is called and its branch above it, which change as the hub's record of it does. What the transcript draws from the chat's records, as the desktop's does, follows them too: whether the agent is working, what it left running, its subagents' calls, its proposals, and which workspace a pull request it links to is in, and going to that workspace from the chip's menu opens its chat. *Unconfirmed.*
- **Return makes a new line, and the arrow sends.** The on-screen keyboard's Return only ever types a new line, so sending is a button. The composer grows to six lines. *Unconfirmed.*
- **What the agent asks, a question or leave to use a tool, takes the composer's place,** in the panel the desktop asks it in, with the answers to tap and a line to type one's own, and the arrow sends it. The composer comes back once it's answered. A plan is approved on its card in the transcript. *Unconfirmed.*
- *Not built:* stopping a turn, other chats in the workspace, the model and mode pickers, the diff, a subagent's or a proposal's own view, a draft kept for what's typed and not sent, which goes when the chat closes, and which chats have a reply nobody has read.

## How it looks

- **It's the desktop's look, at a handheld's sizes,** with the same theme, colours and status marks, and the design system's handheld sizes for text, controls, rows and bars, as the tables in `look.md` give them: body text is 17 points and code 14, in Menlo as on the Mac, and what's tapped is a large control, 44 points tall, as iOS asks. *Not built:* a tap area bigger than what's drawn, so a small control could still be easy to hit. *Unconfirmed.*
- **It's always dark, and portrait only,** like the desktop app, which has no light theme. *Unconfirmed.*
- **The app icon is the desktop's, drawn for iOS:** the working icon's tile filling the canvas, since iOS rounds it, with the fan as big against the tile as on the Mac. `look::phone_icon` draws it, and the gallery checks it against the asset catalog's in `apps/mobile-ios/ios/`. *Unconfirmed.*

![The iPhone app's icon](../../apps/mobile-ios/ios/Assets.xcassets/AppIcon.appiconset/icon-1024.png)

*Not built:* the gallery doesn't draw the phone's screens yet, so none are shown here.
