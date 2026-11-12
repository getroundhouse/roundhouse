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
- Adding a project through the file picker
- `Create PR` on a repository with a GitHub remote opens a real pull request
- Links open, and right-click menus exist where expected
- VoiceOver reads messages and sidebar rows
- Font rendering next to native apps at your usual size
- Status marks: a working agent's light sweeps smoothly at the display's rate, as do the smaller light of one that left something running and the ring of a running background task, nothing else moves, Reduce Motion holds them still, and with every agent idle and nothing in the background the window draws nothing, which Activity Monitor shows as no CPU
- Resize and window snapping stay smooth during streaming
- A long chat: resizing the window stays smooth, the scrollbar's thumb follows the pointer through the whole session and holds the page still while the pointer rests, and scrolling back down to the latest row takes its button away
- Sidebars: each slides open and shut smoothly beside a long chat, its edge follows the pointer and takes its usual width when it's close, with one tap on a Force Touch trackpad as it does, the button beside the window's buttons lines up with them and sits in the corner in full screen, and Reduce Motion skips the slide
- The menu bar's items and keys that macOS carries out: ⌘H and Hide Roundhouse, ⌥⌘H and Hide Others, Show All, ⌘M and Minimize, and Zoom
- With a comment being written, ⌘Q, Quit Roundhouse and the window's close button each ask before quitting, `Keep working` leaves the app as it was, and `Quit` quits
- The strip along the top moves and zooms the window, the header's buttons respond at once, and the window's buttons sit level with the header, in and out of full screen
- The window opens where it was left: filling the screen, in full screen, out of full screen again, and on a second display, each after quitting, after closing the window, and after `Restart to update`
- Settings: the strip along the top still moves the window, its switches respond at once, and going back leaves the workspace as it was, scrolled to the same place
- On Windows, the strip along the top: dragging it moves the window, double-clicking it maximizes the window and restores it, pressing a button in it presses only the button, and letting go of the button off the window doesn't click it, dragging across what's typed in Home's search selects it, and dragging a sidebar's edge from inside the strip resizes the sidebar
- On Windows, the window's buttons: each minimizes, maximizes, restores or closes the window, with a dialog open too, closing with a comment being written asks first, hovering maximize offers Windows' layouts to snap to, the glyphs show on Windows 10 as on Windows 11, Narrator presses each, and the pull request's bar and the header end before them
- Overall: does it feel like an app you want to live in all day?
