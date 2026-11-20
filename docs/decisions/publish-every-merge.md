# Every merge to `main` is published, as version `0.1.<commits on main>`

**Status:** Decided.
**First recorded:** 2026-11-09, in #215.

CI builds, signs and publishes the app on every merge to `main`, and the app updates itself as those builds arrive. So staying current needs no checkout and no toolchain. The merge queue has already run the checks by then.

A published build's version is `0.1.N`, where N is the number of commits on `main`, such as `0.1.50`. Velopack orders updates by it. The version is short and readable, every merge adds one to it, 0.2 and 1.0 stay free to mean something later, and each part fits the 16 bits of a Windows version field.

A commit count can order these builds only because `main` alone publishes, and its history only grows. Counts differ across branches, so a count can't order builds made anywhere else.

## Chosen over

No alternative was recorded.
