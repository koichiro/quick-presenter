# Audience Window

## Role and terminology

Starting with v3.0.0, **Audience Window** is the name of the audience-facing
native window previously called **Slide Window**. Use Audience Window for the
window role in UI labels, current specifications, and implementation APIs.
A slide remains a page of the PDF; slide previews, slide text, and rendering
APIs keep their content-oriented names.

The **Presenter Window** contains controls, notes, previews, and session status.
The **Audience Window** is the output surface for a projector, external display,
or meeting screen share. It displays the current PDF page, black screen, or
Audience Live join guide and enabled reaction/comment overlays. Speaker notes
and presenter controls stay in the Presenter Window. Audience Live is an
optional participation feature; the Audience Window works without enabling it.

## Visible names

| Surface | Name |
| --- | --- |
| Native window title | `Quick Presenter - Audience` |
| Window menu | `Show Audience Window` |
| Window menu | `Hide Audience Window` |
| Window menu | `Bring Audience Window to Front` |
| View menu | `Toggle Audience Fullscreen` |

Native and in-window menus use the same labels and Rust callbacks. macOS keeps
the native title hidden under its existing transparent-titlebar policy, while
retaining the Audience title for native window identification.

## Behavior contract

The rename preserves existing presentation behavior:

- Both windows open at startup. Navigation from either focused window updates
  the shared Rust presentation state and keeps their displayed pages synchronized.
- Closing or hiding the Audience Window leaves the Presenter Window usable.
  Showing or bringing it forward restores the current page and black-screen
  state, including changes made while it was hidden.
- Fullscreen targets only the Audience Window. Existing `F`, `F5`, and `Escape`
  controls remain available; `B` toggles black screen and `X` swaps displays.
- Windowed placement restoration, aspect-ratio fitting, display fallback, and
  platform chrome retain their existing rules. The rename introduces no new
  display-selection or persistence behavior.
- PDF loading, rendering, caching, and errors remain owned by Rust. Slint exposes
  `AudienceWindow`, dimensions, display properties, and event callbacks.

See [keyboard controls](KEYBOARD.md), [fullscreen and native chrome](FULLSCREEN.md),
[display swapping](DISPLAY_SWAP.md),
[window placement and PDF restoration](AUDIENCE_WINDOW_PLACEMENT.md), and
[Audience Live](AUDIENCE_LOCAL.md) for their detailed contracts.

## Compatibility and validation

The startup settings filename, version, and JSON fields remain unchanged.
Existing saved window placement and the last active PDF continue to load without
migration. Control Protocol v1 retains its commands and `fullscreen` field;
that field continues to describe Audience Window fullscreen state.

Historical changelog entries and validation reports retain the terminology used
at the time. Existing screenshots remain labeled with their recorded release;
their filenames and current links use the Audience Window name.

Use the [GUI smoke checklist](GUI_SMOKE_CHECKLIST.md) to verify both menu
implementations, focus, hide/show recovery, fullscreen, display swaps, black
screen, and Audience Live overlays on each supported desktop platform. The
existing Rust tests and GUI smoke harness use the renamed window APIs. A name
change alone does not establish new cross-platform runtime validation results.
