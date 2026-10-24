# Presenter window layout

The presenter window keeps the current slide and notes on the left, the next
slide and keyboard reference in the middle, and slide thumbnails on the right.
Audience Live stays below the first two columns; thumbnails extend to the bottom
of its URL row. The Audience Live toolbar orders the disclosure button, connected audience count
(`👥(0)`), live LED, session toggle, Pause/Resume, server IP address, Next, and
Refresh in one row. The IP address uses its natural text width up to 120 logical
pixels, keeping Next nearby; longer addresses elide. Routine server status
text is omitted; actionable alerts still overlay the history pane. Reactions and
comment history use the full pane width below the toolbar.

Sizing follows the available window space:

- The current and next columns share width at approximately 2:1, with a minimum
  next-column width of 260 logical pixels and no upper cap. Both columns continue
  to grow together on 4K and larger displays.
- The next slide's rendered width is at most half the current slide's rendered
  width. Equal-aspect slides retain a visible 2:1 width ratio even when available
  height constrains the current slide. Mixed-aspect slides also fit within the
  slide row without stretching.
- Thumbnails use 10% of the main area width, bounded to 128–192 logical pixels.
- The slide row uses up to 66% of the main area height, while preserving at least
  200 logical pixels for notes below the heading. A slide that fits at its natural
  aspect ratio releases the remaining height to notes.
- Notes use the entire current-slide column, including when a portrait slide is
  narrower than its column. Existing Rust note fitting and scrolling still apply.
- The next preview scales with its column and retains its aspect ratio within the
  slide row. The keyboard reference remains at most 480 logical pixels wide and 200 high.
- The Audience Live footer uses 28% of window height excluding the inline menu,
  bounded to 282–360 logical pixels, including its controls and URL row.

These are logical pixels; display scaling is still handled by Slint. This layout
introduces no changes to PDF ownership, rendering requests, navigation, or window
placement.

## Validation

`cargo test --test presenter_layout` renders the compiled Slint component with a
software window. It covers eight sizes (800×842 through 5120×2880), three slide
aspect ratios (16:9, 4:3, portrait), and both menu modes: 48 combinations. Assertions
protect usable notes space, rendered slide proportions and bounded thumbnail widths, proportional column
sizing, and separation of the footer and thumbnails. Both expanded and collapsed
footer states are covered (96 layouts in total). Unlike a native resize,
this test can exercise windows larger than the attached display.

To save the rendered layouts for inspection:

```sh
QP_PRESENTER_LAYOUT_SNAPSHOTS=/tmp/qp-presenter-layouts \
  cargo test --test presenter_layout
```

The PPM files show layout with placeholder slides and sample notes, not PDF
rendering or native menu/window chrome. The native GUI smoke also checks the six
window sizes when it reaches the Audience Live layout checks.

Local validation on 2026-10-24: formatting, compilation, the complete Cargo test
suite, and the 48 software-rendered layouts passed. The 1920×1080 landscape and
800×842 portrait/inline-menu images were visually inspected. The signed renderer
PDF/notes and sandbox denial gate passed during bundle signing. The full macOS
GUI smoke stopped before layout checks at rapid fullscreen-entry cancellation
(`will-enter` without completion), both from a direct bundled launch and through
Launch Services. This does not establish native fullscreen, Windows, or Linux
runtime qualification for the layout change.

The large-display follow-up removes the next-column maximum width and checks
the actual rendered current/next slide widths at 2:1 for matching aspect ratios,
rather than accepting column proportions alone. All 48 software layouts passed;
a maximized signed local app with a real PDF was also inspected and captured.
The keyboard reference retains its 480-pixel width cap and left alignment.

The Audience Live toolbar follow-up checks count/LED/address control order and
full-width history at all 48 sizes/aspect/menu combinations, including a five-digit
audience count and a long IPv6 address at the minimum window width. The audience
icon is a bundled vector so that it does not depend on color emoji font support.

Audience Live history and its URL row start collapsed, leaving the toolbar visible.
Starting a session opens them; finishing a normal stop collapses them. The disclosure
button before the audience count uses a boxed plus when collapsed and a boxed
minus when expanded, with no background fill in either state. A focus outline
appears during keyboard interaction, and Space/Return activate the button. Its tooltip
names the available action. It can open or close the pane independently of session,
Pause/Resume, or join-screen state. Manual choices survive live event polling until
the next session transition. In a normal window, collapsing shortens the window by the hidden history/URL
height, and expanding adds that height back. Slides and notes retain their space.
The equivalent expanded footer height is inferred while collapsed, including
when the presenter resizes the collapsed window. The minimum/default client
heights are 608/768 logical pixels collapsed and 842/1002 expanded. The thumbnail
strip ends alongside the toolbar instead of the hidden URL row. Maximized and
fullscreen windows retain their OS-controlled geometry.
New configuration/start/session failure alerts open the pane once so recovery
guidance remains visible; the disclosure button can subsequently close it again.

Local validation on 2026-10-25 for collapsible history: formatting, compilation,
the complete Cargo suite, two panel-visibility unit tests, and all 96 rendered
layouts passed. In a signed local macOS app with a real PDF, verified initial
collapse, manual opening while inactive, automatic opening on a local session
start, manual collapse/reopen while that session stayed active across polling,
and automatic collapse after stopping. The session was stopped after validation.
The signed XPC PDF/notes and sandbox denial gate also passed. This focused manual
check does not replace the outstanding full native fullscreen smoke or runtime
qualification on other platforms.

The window-height follow-up additionally passed all 96 layouts using the production
geometry controller, repeated disclosure at 1.0/1.25/1.5/2.0 scaling, and expansion
after manually resizing while collapsed. Exact device-pixel size pairs prevent
rounding drift across repeated toggles. In the signed local macOS app, screenshots
measured 1024×800 collapsed, 1024×1034 expanded, and 1024×800 collapsed again
(including the native titlebar), with the same slide and notes bounds. Local session
start and stop produced the same height changes, and the session was stopped after
validation. Formatting, compilation, and the complete Cargo suite passed.

The unfilled disclosure button was checked in the signed local macOS app: pointer
activation, Space, Return, and Tab traversal all worked without changing the PDF
page. Both collapsed and expanded screenshots show only the boxed icon, with no
button background fill. The complete Cargo suite and 96 layout cases passed.
