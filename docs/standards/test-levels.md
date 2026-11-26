# Test levels

Where each kind of test goes, and the hub a test of the hub or the window runs on. How a test is written, and what it runs in, is in `docs/standards/testing.md`.

| Level | Tests | Where |
|---|---|---|
| A module | Its own rules, without a process or a socket. One that runs a program tests its rules without it, and starting, watching and stopping one against a stand-in | Unit tests |
| The hub | What the hub decides by itself, with stand-ins for its workers | Unit tests in `crates/hub/` |
| The agent adapter | Normalizing the CLI's captured output | `crates/agents/tests/` |
| The whole backend | What spans processes: restarts, crashes, a real worker, with a fake agent | `apps/daemon/tests/` |
| The window | Clicking and typing through to what's shown | `apps/desktop/tests/` |
| How each screen looks, and the app icon | That each still matches its approved image | `apps/desktop/tests/ui/gallery/` and `apps/mobile-ios/tests/ui/gallery/` |
| The repo's rules | Dependencies, licenses, file length and lint inheritance, against the real dependency graph | `tools/policy/` |
| The design's tool | That it lists each canvas of the design with its status, and that `design/Index.bl` is as the badges have it | `tools/design/` |
| The guide's tool | That it publishes each page and refuses a link, heading or picture that isn't there, against made-up guides. CI's `guide` job builds the real one | `tools/guide/tests/` |
| The kit's tool | That it makes the copy of gpui-kit and keeps its patches in step with it, against stand-ins for upstream's repository and this one | `tools/kit-sync/tests/` |
| Publishing updates | That a release signs only a feed it computed, finishes after stopping partway, and reads back as apps would, against a directory standing in for the bucket | `tools/package/test_publish.py` |
| Dictation's hint words | Which terms each made-up case's field hints, against a baseline | `tools/dictation-eval/` |
| Dictation's model | How it hears real recordings, with hints and without, against a baseline. By hand on a Mac, since the model runs only there | `tools/speech-eval/` |
| An agent's CLI | That a version of it does what Roundhouse needs, as `docs/reference/conformance.md` says | `crates/agents/src/conformance/` |
| The real CLI | That the fake agent is still honest. By hand, since it costs money | `tools/smoke/` |

- **A rule the hub decides by itself is a unit test in the hub,** on a hub from `hub_with_stand_in_workers`, whose stand-in answers as a worker would and says what it was asked.
- **The window is tested through its real UI,** by clicking and typing in the kit's headless windows, which keeps test code out of the product and catches what a picture can't, such as a click that does nothing.
- **A window test opens on a fake hub, with `Ui::open_on`, unless it's about what a hub, a worker or an agent does.** What the window draws of what a hub holds needs none of them, and real ones make it slower and give it more to race. On a real hub, only the agent is faked.
- **The fake hub answers by the hub's own rules,** so it sends only what a hub could. It calls the rule in `protocol` that the hub calls. Where the hub's store does it in SQL, `protocol` holds the rule behind `testing`, and the store's tests hold the SQL to it.
- **The fake hub is in `client_core`, behind `testing`, not beside the gallery,** which only a Mac builds, so a protocol change it doesn't follow fails CI on Linux.
- **The gallery draws each screen from the fake hub's fixed data,** since a real hub's IDs, times and timing differ from run to run, and a picture has to come out the same. What a picture shows, and how one is accepted, is in `docs/spec/README.md`.
