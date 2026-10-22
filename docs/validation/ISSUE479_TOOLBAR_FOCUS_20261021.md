# Presenter toolbar shortcut investigation — 2026-10-21/22

Tracking: #507 (macOS observation split from closed #479). Candidate: draft #506.
The Windows notes-navigation code fix was merged separately in #503.

## Candidate and existing checks

The candidate places a capture-only FocusScope around toolbar descendants and
classifies presentation shortcuts in Rust before button handling. The scope is
excluded from click and Tab focus. Accepted commands use existing keyboard
callbacks. Space/Return retain focused-button activation; Tab/Shift+Tab retain
traversal. Notes remain outside the scope. This is a proposed precedence path,
not a confirmed explanation or necessary fix for the historical Mac observation.

Candidate checks previously passed formatting, check, the complete Cargo suite,
three shortcut-classification tests, release build, and the signed XPC PDF
render/notes and file/network/child denial gate. The existing fetch_update
warning was unrelated. Local signing used timestamp=none.

Windows candidate native regression at executable source e1b0b4d passed toolbar
traversal/activation, Black-focused presentation keys, and note selection/copy.
The complete serial Cargo suite passed (416 main tests and all 14 renderer
process tests). The later 1d2b21f change was documentation only. These are Windows
candidate results, not Mac reproduction or proof of a shared cause. The
notes-to-Open traversal boundary was not compared against its baseline.

## Earlier Mac baseline

A toolbar-unmodified signed baseline from #504 source 14484d4 (main 430cef0 plus
fullscreen changes) did not reproduce the old Home/B observation: seven Tabs
from the current slide focused Black; after its activation, Home returned page
5 to 1 and B cleared blackout while AX still reported Black focused. The
candidate comparison was interrupted by native-tool stalls and desktop lock.

## Current-main Mac check — 2026-10-22

Source: main c662db1f255d70271fb2f6e2e5a493467b4cf6e8 (2.5.0), including merged
#503 and #504, without #506. The toolbar and ordinary KeyBindings are unchanged
from historical v2.0.0 source 368e5ec71df65c64c790eff0fa7d3de1ac31333e.

Environment: macOS 27.0.1 arm64; 12-page quick-presenter-demo.pdf, windowed.
A normal installed app was already running, so the validation copy used a
separate UI bundle ID (app.quickpresenter.focusvalidation.main507), private IPC
runtime directory, and a temporary configuration-directory replacement. The
latter redirects only platform_config_dir() to /private/tmp/qp507-main-config;
its source edit was reverted after building. Toolbar and key handling were not
changed. The installed application and its settings were not manipulated.

Signed GUI SHA-256:
`9712e3ccdf9a4824485104d54ac638167f8cf647dea9d9c3ac40296278a1fe05`.
An unmodified main signed build was also prepared, SHA-256
`848675b52abf67078e077c5b9ab3d04a198beafd090e7ac37f66c7df72b587dc`;
it was not used for these interactive checks. Both layouts passed signed XPC
render/notes and file/network/child denial checks with PDFium 156.0.8076.0.
These local artifacts are not final notarized distributions.

### Native injected input results

After unlocking, selecting the validation app by its full bundle path worked.
From the current-slide image, Tab focused Open; six further Tabs focused Black.
Both AX focus and the visible keyboard focus outline identified Black. Its
Space/Return behavior provided an independent functional focus check.

| Case | Observed result |
| --- | --- |
| Black focused, page 5, blackout off: Home then B | Page 5 → 1; blackout enabled; AX focus stayed on Black. |
| Black focused: Space then Return | Blackout disabled then enabled; page remained 1. These activated the button rather than presentation Next. |
| After those activations, CLI sets page 5 without changing focus: Home then B | Page 5 → 1; blackout disabled; AX focus stayed on Black. |
| Black focused: End, Left, Right | Page 1 → 12 → 11 → 12; AX focus stayed on Black. |
| Click current-slide image, then Space | Page 5 → 6 with blackout off, verifying ordinary presenter focus by behavior. |
| Recreate original toolbar history with Tab/Space: First, Next, Last, Black; then Home and B | Pages 6 → 1 → 2 → 12; Black activation enabled blackout; Home moved 12 → 1 and B disabled blackout. AX focus stayed on Black for Home/B. |

Home/B non-response was **not reproduced in three tested sequences**. Independent
CLI status confirmed page 1, blackout off, windowed and renderer ready after the
last sequence. No exact animation timing or physical-key result is inferred.
The validation app was then quit through its own application menu; its process
exited successfully. No application source change was committed.

### Control-shortcut ambiguity

Injected ctrl+a at page 12 and ctrl+e at page 5 did not change pages with Black
focused. After the image click and the successful ordinary-focus Space check,
both injected combinations also did not change page 6. This does not isolate a
toolbar-focus defect; event delivery, modifier translation and binding behavior
were not separated in this attempt. Do not call the injection tool the confirmed
cause or claim a new physical-key defect. The maintainer's earlier physical
Ctrl+A/E success on main c5c29cd remains separate evidence; physical keys on
c662db1 were not tested.

## Disposition

The current result does not justify merging #506 as a fix for Home/B on Mac.
Keep #506 draft and #507 open for a reproducible affected focus/input state and
physical-key comparison. Notes interactions and the exact historical published
artifact have not been qualified by this run.

The official v2.0.0 DMG was downloaded and its SHA-256 matched the historical
report (72fe9c71e7874aacc75563d28dad34b11ac7c73a05a8f36abe56deb79f9b553d),
but no native input sequence was run on it in this attempt. Previous long native
tool stalls and locked-desktop failures are environment failures, not shortcut
reproductions. Raw smoke or UI-tool output is not committed.
