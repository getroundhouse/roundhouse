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
- With 1Password's SSH agent and a repository whose remote is over SSH, an agent's push brings up 1Password's prompt at once, naming Roundhouse Hub, and goes through once it's allowed
- Links open, and right-click menus exist where expected
- VoiceOver reads messages and sidebar rows
- Font rendering next to native apps at your usual size
- Status marks: a working agent's light sweeps smoothly at the display's rate, as do the smaller light of one that left something running and the ring of a running background task, nothing else moves, Reduce Motion holds them still, and with every agent idle and nothing in the background the window draws nothing, which Activity Monitor shows as no CPU
- Resize and window snapping stay smooth during streaming
- A long chat: resizing the window stays smooth, the scrollbar's thumb follows the pointer through the whole session and holds the page still while the pointer rests, and scrolling back down to the latest row takes its button away
- Sidebars: each slides open and shut smoothly beside a long chat, its edge follows the pointer and takes its usual width when it's close, with one tap on a Force Touch trackpad as it does, the button beside the window's buttons lines up with them and sits in the corner in full screen, and Reduce Motion skips the slide
- The menu bar's items and keys that macOS carries out: ⌘H and Hide Roundhouse, ⌥⌘H and Hide Others, Show All, ⌘M and Minimize, and Zoom
- With something typed in the new-workspace dialog while the hub is stopped with `kill -STOP`, so it can't keep it, ⌘Q, Quit Roundhouse and the window's close button each ask before quitting, `Keep working` leaves the app as it was, and `Quit` quits. `kill -CONT` lets the hub carry on
- The strip along the top moves and zooms the window, the header's buttons respond at once, and the window's buttons sit level with the header, in and out of full screen
- The window opens where it was left: filling the screen, in full screen, out of full screen again, and on a second display, each after quitting, after closing the window, and after `Restart to update`
- Settings: the strip along the top still moves the window, its switches respond at once, and going back leaves the workspace as it was, scrolled to the same place
- On Windows, the strip along the top: dragging it moves the window, double-clicking it maximizes the window and restores it, pressing a button in it presses only the button, and letting go of the button off the window doesn't click it, dragging across what's typed in Home's search selects it, and dragging a sidebar's edge from inside the strip resizes the sidebar
- On Windows, the window's buttons: each minimizes, maximizes, restores or closes the window, with a dialog open too, closing with a comment being written asks first, hovering maximize offers Windows' layouts to snap to, the glyphs show on Windows 10 as on Windows 11, Narrator presses each, and the pull request's bar and the header end before them
- On Windows and Linux, the keys: Ctrl+N, Ctrl+T, Ctrl+W, Ctrl+F, Ctrl+G, Ctrl+Shift+D, Ctrl+, and Ctrl+Q each do what they do with ⌘ on a Mac, the Windows or Super key with them does nothing of the app's, F3 and Shift+F3 step through what's found, and in the composer Ctrl+A, Ctrl+C, Ctrl+X, Ctrl+V, Ctrl+Z, Ctrl+Y and Ctrl with an arrow edit the text, and on Linux with IBus, Ctrl+Shift+U and Ctrl+. reach the app while the composer has focus, rather than starting Unicode entry or the emoji picker
- On Windows and Linux, in a terminal: Ctrl with a letter, Ctrl+, , Ctrl+., F3 and F10 reach the program, as in a terminal app there; Ctrl+Shift+W, Ctrl+Shift+N, Ctrl+Shift+Q, Ctrl+Shift+T and Ctrl+Tab are the app's; and Ctrl+Shift+C, Ctrl+Shift+V and Ctrl+Shift+A copy, paste and select all
- In a terminal on a Mac: ⌘← and ⌘→ go to the start of the shell's line and to its end, ⌘⌫ deletes back to its start in bash, or the whole line in zsh, and ⌥← and ⌥→ move by a word, as in Ghostty; and selecting with a drag, a double click and a triple click feels as in a terminal app, and ⌘C copies it
- On Windows and Linux, the menus: the button in the top left corner opens them under it as it's pressed, and stays where it is with the sidebar hidden and over Settings; each menu opens as a submenu under the pointer and with the arrow keys; F10 opens and closes them, Escape closes them with the cursor back where it was, and a click elsewhere closes them; Tab reaches the button, and Enter or Space opens them; an item that can't act is greyed out; each item does what its key does, and shows the key as Ctrl+; Edit's items edit the text box that had focus; Exit or Quit, Check for Updates… and About work; a screen reader reads the items; and on Windows the menus can be pressed where they cross the strip, which doesn't move the window
- Overall: does it feel like an app you want to live in all day?
