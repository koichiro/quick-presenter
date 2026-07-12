# Keyboard Controls

Quick Presenter accepts presentation navigation keys in both windows when that
window has keyboard focus. The Rust input layer maps key callbacks to a shared
presentation command path, so button navigation and keyboard navigation update
the presenter and slide windows together.

## Presenter Control Focus

- Move to the next presenter control: `Tab`
- Move to the previous presenter control: `Shift+Tab`
- Activate a focused inline menu heading, menu item, or thumbnail: `Return` or
  `Space`

When a presenter control consumes `Return` or `Space`, it activates that
control instead of advancing the presentation. Presentation shortcuts continue
to use the shared command path when no focused control consumes the key.

## Page Navigation

- Next page: `Space`, `Return`, `Right Arrow`, `Down Arrow`, `Page Down`
- Previous page: `Left Arrow`, `Up Arrow`, `Page Up`, `Backspace`
- First page: `Home`, `Ctrl+A`
- Last page: `End`, `Ctrl+E`

Navigation is clamped by the presentation state. Pressing a next key on the last
page or a previous key on the first page leaves the current page unchanged.
First-page and last-page jumps use the same shared presentation command path as
normal next/previous navigation, so the presenter and slide windows stay
synchronized. `Ctrl+A` and `Ctrl+E` provide Emacs/readline-style alternatives
for keyboards where `Home` and `End` are harder to reach.

## Fullscreen

- Toggle slide fullscreen: `F5`, `F`
- Exit slide fullscreen: `Escape`

Fullscreen shortcuts are handled through the same shared fullscreen state as the
presenter fullscreen button. `Escape` exits slide fullscreen without toggling it
back on.

## Black Screen

- Toggle black screen mode: `B`

Black screen mode blanks only the audience-facing slide window. The presenter
window remains usable, and page navigation continues while the audience screen
is black. Pressing `B` again reveals the current page, including any page
changes made while the slide window was blanked.

Future presentation controls such as go-to-page and timer actions should be
added as new Rust presentation commands before adding UI key bindings.
