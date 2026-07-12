# Accessibility Baseline

Quick Presenter v1.0.0 targets an accessibility baseline for the core live
presentation workflow. This baseline is not a claim of conformance with a
particular accessibility standard. It defines the behavior that release
testing must preserve for presenters who use a keyboard or assistive
technology.

## Supported Workflow

The presenter must be able to complete these actions without a mouse:

- open a PDF through the application menu;
- use the application menus;
- move to the previous, next, first, or last slide;
- focus and activate a slide thumbnail;
- enter and exit slide fullscreen;
- enable and restore black-screen mode; and
- reach and read speaker notes.

The existing presentation shortcuts remain available whenever an interactive
presenter control does not consume the key. When a custom control has keyboard
focus, `Return` or `Space` performs its default action. `Tab` and `Shift+Tab`
move between focusable controls.

See [KEYBOARD.md](KEYBOARD.md) for presentation shortcuts.

## Exposed Semantics

Custom presenter controls expose the following Slint accessibility semantics:

| Control | Role | Name | State and action |
| --- | --- | --- | --- |
| Inline menu heading | `button` | Visible menu title | Expanded state and default activation |
| Inline menu item | `button` | Visible item title | Enabled state and default activation |
| Thumbnail pane | `list` | `Slide thumbnails` | Item count |
| Thumbnail tile | `list-item` | Page label | Selectable, selected/current state, and default activation |
| Speaker notes | `groupbox` | `Speaker notes` | Note text as its description |

The current thumbnail also includes a visible `CURRENT` label. Keyboard focus
uses a high-contrast border, so neither state relies on color alone.

## Platform Limitations

Quick Presenter uses native menus on macOS and Windows. Their keyboard and
assistive-technology behavior is provided by the operating system and must be
verified in a packaged build.

Linux uses the Slint inline menu fallback because the native menu bar is not
compiled on that target. Slint 1.16 does not provide `menu` or `menu-item`
accessible roles, so inline menu headings and items are exposed as buttons.
They still provide names, enabled or expanded states, default actions, and
keyboard focus.

Accessibility trees and screen-reader announcements depend on the Slint winit
backend and the platform accessibility bridge. Automated unit tests cannot
validate OS announcements, native focus behavior, or the ordering presented by
screen readers. These remain release smoke checks.

## Display Scaling

Release testing covers 125%, 150%, and 200% display scaling. At each scale:

- menu labels and focus borders must remain visible;
- thumbnails, current-state text, and scrollbars must remain usable;
- current slide, next preview, timer, clock, status, and notes must not overlap;
- the About dialog must fit within the presenter window; and
- long recent-file names and notes must remain reachable without resizing the
  application beyond the available work area.

`SLINT_SCALE_FACTOR` can help reproduce scale-sensitive layout issues during
development, but it does not replace testing the packaged application with the
operating system's display scale.

## Release Validation

Use these platform tools where feasible:

- Windows Narrator and Accessibility Insights;
- macOS VoiceOver and Accessibility Inspector; and
- Linux Orca over AT-SPI on the supported Ubuntu desktop target.

Record the operating system, display server where applicable, display scale,
screen reader or inspector version, application artifact, and any unsupported
or incorrectly announced state. A failure in the supported workflow is a
release blocker unless it is explicitly accepted and documented for the
release.
