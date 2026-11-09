# Manual checks

What a person has to judge on the desktop app, because feel and OS integration can't be measured from inside it. Run an installed build, as the README describes, because a debug build's own crates aren't optimized. Score each check Pass, Weak or Fail, and put the results in an issue.

Start with trackpad feel, then IME, then selection and copy.

- Trackpad scrolling feels native: momentum, rubber-banding, no lag
- Text selection within a message feels right, including double and triple click
- Selection across several visible messages, then copy
- Selection extended past the viewport by dragging, then copy
- Copied text from a code block and a table pastes sensibly
- Find in transcript works
- Composer: IME input (try Japanese or Chinese), emoji picker, dictation
- Composer: Option/Cmd arrow movement, undo, spellcheck
- Image paste into the composer
- Adding a repository through the file picker
- `Create PR` on a repository with a GitHub remote opens a real pull request
- Links open, and right-click menus exist where expected
- VoiceOver reads messages and sidebar rows
- Font rendering next to native apps at your usual size
- Status marks: a working agent's light sweeps smoothly at the display's rate, nothing else moves, Reduce Motion holds it still, and with every agent idle the window draws nothing, which Activity Monitor shows as no CPU
- Resize and window snapping stay smooth during streaming
- The strip along the top moves and zooms the window, the header's buttons respond at once, and the window's buttons sit level with the header, in and out of full screen
- Overall: does it feel like an app you want to live in all day?
