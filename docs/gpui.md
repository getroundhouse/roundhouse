# GPUI

How code that uses GPUI is written. `docs/architecture.md` says which crates may use it, `docs/rust.md` covers everything else about how Rust is written here, and `docs/ui.md` covers why GPUI and gpui-kit were chosen.

- **`render` does no I/O or blocking.** It reads state the view already holds.
- **Calls into `client-core` go through `cx.background_spawn`,** never on the main thread, because they block.
- **A long `render` splits by region of the screen,** into functions that return elements.
- **Nothing imports `gpui_kit::*`.** A glob brings in GPUI's `test` attribute, which makes `#[test]` recurse. The lint against glob imports catches it.
