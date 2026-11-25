# Connect to another machine's hub

Use the hub switcher above Home in the sidebar to choose this device or a saved hub. The workspace list, chats and changes belong to the selected hub. Switching leaves agents running on their own machine, preserves each hub's open panes and drafts in the window, and shows how many workspaces need a look on background hubs.

Add projects on the machine that owns the hub. The remote workspace list then shows them here. A background hub's dialogs wait until you switch back to it.

## Pair once

1. Install a build with pairing support on both machines. Keep the hub's machine awake and reachable over your local network or VPN, such as Tailscale.
2. On that machine, open the hub switcher and choose **Pair a device with this Mac…**, then **Copy link**.
3. On this machine, open the hub switcher and choose **Connect another hub…**.
4. Paste the `roundhouse://pair?…` link, check the previewed hub name and addresses, and choose **Connect**.

The link expires after ten minutes and works once. Once this machine connects, the other machine's dialog confirms the paired device by this computer's name. A paired device can do what you can in the app on the hub's machine: work in its workspaces, change the hub's settings and accounts, and open a terminal there. So share a link only with a device you trust with that machine. The app saves the connection and returns to the selected hub when reopened. Its token stays in the operating system's credential store.

Choose **Create new link** if the link expired before a device connected. Each link keeps its own expiry; creating another does not invalidate earlier links. You can also generate a link and QR code from a terminal on the hub's machine with `roundhouse phone pair`.

## When it cannot connect

The selected hub shows **Offline**, **Access revoked**, **Key changed** or **Update required** in the switcher, and explains why in place of the workspace when none is open. The app tries again by itself, at once and then after longer and longer waits, up to 30 seconds, without closing a workspace that's open. Only **Access revoked** and **Key changed** wait for you, and **Update required** does when it's this app that needs updating. Check that the other machine is awake, its hub is running, and the network allows the port: 7867 in installed builds, or 7868 in development builds. `ROUNDHOUSE_LISTEN` on the hub's machine chooses another port or a specific address. There is no relay that connects machines across otherwise unreachable networks.

If the hub reports a different key, check that you are connecting to the intended machine before choosing **Pair again…**. An expired, used or revoked credential also offers **Pair again…** to paste a fresh pairing link. **Update required** means this app and the hub's Roundhouse are too far apart to connect, and the app says which one to update:

- **The other machine needs updating.** Update Roundhouse there. If its app is open, choose **Restart to update**. If its hub is [installed as a service](../reference/updates.md#a-hub-on-a-machine-of-its-own), run `roundhouse hub update` on that machine, more than once if it's several updates behind. This app reconnects by itself.
- **This app needs updating.** Choose **Restart to update**, or **Check for updates** if nothing is downloaded yet.

You can't restart another machine's hub from here.

Pairing again with the same hub key replaces its saved authorization and reconnects the existing view, including drafts waiting to be saved. A different key creates a separate saved hub.

## Revoke a device

On the hub's machine, run `roundhouse phone list`, then `roundhouse phone revoke NAME-OR-ID`. Revoking closes that device's connections and prevents it from reconnecting. It does not stop the hub's agents.
