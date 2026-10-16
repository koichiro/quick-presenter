# Candidate desktop observations — 2026-10-16 JST

Exact package identity and SHA-256: see the parent validation report.

## Maintainer confirmation

The maintainer was asked to test two-display X switching, fullscreen,
disconnect/reconnect recovery and keyboard operation after recovery, and
replied that normal operation was confirmed. These checks used the extracted
candidate launched with `docs/samples/quick-presenter-demo.pdf`. The maintainer
asked the agent to perform the separate screen-reader check.

## Windows Narrator

Method: start the preinstalled Windows Narrator, open Speech recap with
Insert+Alt+X, move to the first app item with Insert+Home and traverse items
with Insert+Right. Speech recap was visually inspected. This uses the
[Microsoft-documented Narrator commands](https://support.microsoft.com/en-us/accessibility/windows/narrator/appendix-b-narrator-keyboard-commands-and-touch-gestures).

App-specific phrases observed in the Japanese Narrator transcript:

- Quick Presenter: region/window and title bar.
- File, Navigate, View, Window, Help: menu items.
- Open, First Page, Previous Page, Next Page, Last Page, Fullscreen, Black:
  button names and roles were spoken.
- `quick-presenter-demo.pdf`, `00:00`, clock value, `1 / 12`, and Notes.
- Read-only edit text beginning with the demo's English speaker notes:
  “Open with the product promise: Quick Presenter does one thing well”.
- Slide and preview objects: generic image announcements; spoken slide content
  was not established.

UI Automation independently exposed the same button names, notes value and
page/status text. This does not prove every custom thumbnail/menu state,
dynamic update announcement, keyboard activation or scaling combination.
Broader accessibility acceptance remains #339.

## Notes-focus keyboard limitation

After the Narrator check and exit, UI Automation reported the read-only notes
as focused. On the demo deck:

| Target and input | Observed page |
| --- | --- |
| Presenter Home, then Space | 1, then 2 |
| Presenter Return with notes focused | Stayed at 2; expected 3 for presentation navigation |
| Slide window Return | Advanced to 3 |
| Presenter Right with notes focused | Stayed at 3; expected 4 for presentation navigation |

The no-advance results and slide-window recovery were independently checked
with the matching CLI's Status. This is a bounded injected-native-key
observation, separate from the maintainer's physical display/recovery pass.
It establishes a notes-focus limitation to retain under #339, not a complete
diagnosis of Slint's input dispatch. Focus the slide window for the observed
working navigation path. No production fix or release waiver is claimed.

The automation helper intermittently failed window activation/click/capture,
and its separate subprocess context could not access the authenticated control
pipe (access denied). The shell-based CLI matrix had already passed. No
product defect or pass was inferred from those helper failures. Narrator item
navigation and the actual app-specific speech transcript were obtained despite
those tool limitations. Narrator was exited after testing.

No unrelated desktop speech, screenshots, or user content are published here.
