# Keyboard Controls

Quick Presenter accepts presentation navigation keys in both windows when that
window has keyboard focus. The Rust input layer maps key callbacks to a shared
presentation command path, so button navigation and keyboard navigation update
the presenter and slide windows together.

## Page Navigation

- Next page: `Space`, `Return`, `Right Arrow`, `Down Arrow`, `Page Down`
- Previous page: `Left Arrow`, `Up Arrow`, `Page Up`, `Backspace`
- First page: `Home`
- Last page: `End`

Navigation is clamped by the presentation state. Pressing a next key on the last
page or a previous key on the first page leaves the current page unchanged.
First-page and last-page jumps use the same shared presentation command path as
normal next/previous navigation, so the presenter and slide windows stay
synchronized.

## Fullscreen

- Toggle slide fullscreen: `F5`, `F`
- Exit slide fullscreen: `Escape`

Fullscreen shortcuts are handled through the same shared fullscreen state as the
presenter fullscreen button. `Escape` exits slide fullscreen without toggling it
back on.

Future presentation controls such as black screen, go-to-page, and timer actions
should be added as new Rust presentation commands before adding UI key bindings.
