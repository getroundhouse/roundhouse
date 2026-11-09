# The iPhone app

`docs/spec/mobile.md` says what it does. It's a Rust binary, `roundhouse-ios`, drawn with GPUI's iOS backend and the same kit, theme and transcript as the desktop app. `docs/ui.md` says why. Everything but the start-up in `src/main.rs` also builds on the Mac, which is where its tests run.

## Testing

```sh
cargo test -p mobile-ios                                    # The app in a phone-sized window, against a real hub with the fake agent
cargo test -p desktop --test ui the_phone_icon              # The app icon, against the asset catalog's
IPHONEOS_DEPLOYMENT_TARGET=17.0 cargo clippy -p mobile-ios --target aarch64-apple-ios -- -D warnings   # What only builds for the phone
```

The UI tests tap what someone would tap. The app pairs and connects as it does on a phone, over TLS, to a hub that listens only on loopback, so no firewall asks. What only runs on a phone, such as the Keychain and the Camera app opening a link, is checked by hand.

## Running it on a phone

It needs Xcode, [XcodeGen](https://github.com/yonaskolb/XcodeGen), the iOS target (`rustup target add aarch64-apple-ios`), and a phone with Developer Mode on, connected to this Mac by cable or on its network.

```sh
TEAM_ID=<team> BUNDLE_ID=<id> DEVICE=<udid> apps/mobile-ios/scripts/build.sh
```

`TEAM_ID` is the Apple Developer team that signs it, `BUNDLE_ID` the app's identifier in that team, and `DEVICE` the phone's identifier from `xcrun devicectl list devices`. None of them are kept in the repo, since the product's name, domains and accounts are kept privately. The script builds the binary in release, has Xcode sign it, and installs and opens it. Xcode manages the signing, so the first build can register the phone and the identifier in that team's account. A locked phone gets the app but doesn't open it.

It builds in release because the first layout of a debug build overflows the 1 MB stack iOS gives the main thread. `--debug` builds one anyway, and `--no-install` stops once it's signed.

## Pairing with a hub

The phone reaches a hub over the network, so the hub has to listen for phones, and `roundhouse phone pair` sets that up and shows the QR code. Scan it with the Camera app, which offers to open Roundhouse. `roundhouse phone list` shows the paired phones, and `roundhouse phone revoke` removes one.

From a checkout, `cargo run -p cli -- phone pair` pairs with the checkout's own hub, in `.context/dev`, so a phone used for development never reaches the hub of the app used every day.

## How it's put together

- **The Rust binary is the app.** `ios/project.yml` describes an app whose only source is a stub, so that Xcode signs it, and its last build phase puts the Rust binary in the stub's place. XcodeGen makes the Xcode project from it on each build, and the project isn't kept.
- **`ios/Info.plist` says what iOS should know before the app runs:** the `roundhouse` link scheme for pairing, why it reaches the local network, GPUI's scene delegate, the 120 Hz opt-in, and the launch colour. Each has a comment.
- **`ios/PrivacyInfo.xcprivacy` declares what App Store Connect asks of every app:** nothing collected, no tracking, and why it calls the file APIs Apple asks a reason for. A new dependency can call others, which `nm -u` on the binary shows.
- **GPUI's iOS backend is patched,** in `third_party/gpui-pre-ios/`, through `[patch.crates-io]` in the root `Cargo.toml`. Each change is marked `Changed for Roundhouse`: asking the display for 120 Hz, and giving the keyboard a document range from an input that reports none, without which it never appears for the kit's inputs. Upgrading GPUI means copying the new release there and making both changes again.
- **The icon is drawn in code, like the desktop's.** `look::phone_icon` draws it, and the gallery checks it against `ios/Assets.xcassets/AppIcon.appiconset/icon-1024.png`, kept without an alpha channel, since App Store Connect refuses one with it.

*Not built:* archiving for TestFlight and the App Store.
