# GPUI

How code that uses GPUI is written. `docs/architecture.md` says which crates may use it, `docs/rust.md` covers everything else about how Rust is written here, and `docs/ui.md` covers why GPUI and gpui-kit were chosen.

- **`render` does no I/O or blocking.** It reads state the view already holds.
- **Calls on a `client-core` `Client` never run on the main thread,** because they block. Reading or updating what client-core keeps, in a `RecordsFeed` or an `OpenSession`, is fine there, because neither holds its lock while it waits on the hub.
- **A call that changes something goes through `Client::start`, and one that only reads through `cx.background_spawn`.** `start` sends the call at once, on a thread of its own, and returns a `Reply` for a task to await, and the app waits for what it started before it quits. A task only starts on a later turn of the main loop, which quitting doesn't give it, so a prompt sent from one just before quitting never arrives. A read lost that way costs nothing. *Unconfirmed.*
- **What quitting waits for doesn't need the main thread,** because quitting holds that thread while it waits, for at most GPUI's 200 ms. So it waits on a channel that another thread answers, as `Client::start` and saving settings do, rather than on a task. *Unconfirmed.*
- **A long `render` splits by region of the screen,** into functions that return elements.
- **Nothing imports `gpui_kit::*`.** A glob brings in GPUI's `test` attribute, which makes `#[test]` recurse. The lint against glob imports catches it.
