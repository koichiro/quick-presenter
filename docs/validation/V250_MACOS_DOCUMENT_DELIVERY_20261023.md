# v2.5.0 final macOS document delivery follow-up — 2026-10-23

Tracking: [#508](https://github.com/koichiro/quick-presenter/issues/508).
This completes additional document-delivery coverage after the
[final runtime report](V250_FINAL_MACOS_RUNTIME_20261023.md) merged through #514.
The final source, features, entitlements and package bytes are unchanged.

## Apple-distributed TestFlight 25010

The exact `/Applications/Quick Presenter.app` installed by TestFlight was used,
with build **25010**, production bundle identity and the existing sandbox.

| Route | Result |
| --- | --- |
| Warm Finder Open With > Other | Selected the exact installed app; `Warm Finder.pdf` opened at **1 / 3**. Native Right advanced to **2 / 3**, with the English speaker note. |
| Cold Finder Open With > Other | Normally quit the app, selected the exact app for `Cold Finder.pdf`; opened at **1 / 2**, taking precedence over the remembered warm deck. Native Right advanced to **2 / 2**, with the Beamer speaker note. |
| Finder PDF → installed Store app bundle icon | Maintainer confirmed `Icon Drop.pdf`, **1 / 3**; native accessibility also showed the expected file/page and Ready. |
| PDF → running Store Dock icon | Maintainer confirmed `Dock Drop.pdf`, **1 / 2**. |
| PDF → presenter window | Already confirmed under #514: `Presenter Drop.pdf`, **1 / 2**. |
| PDF → audience/slide window | Maintainer confirmed `Audience Drop.pdf`, **1 / 2**. |

The Finder chooser's Always Open With option remained unchecked. The default
PDF handler is still Preview. All four Store manual drop routes are now covered;
these results do not imply customer Store, other-user or other-OS qualification.

## Final direct DMG comparison

Selected notarized/stapled DMG SHA-256:
`5f1ae4ee19299ad3aa136a9a73d814ec352624cb124ca13b7632aae0cc6898b0`.
Its app and the installed direct validation app still match in all **112**
regular-file hashes. Both use the same production bundle identity and PDF
`com.adobe.pdf`/Viewer/Alternate declaration as Store 25010. Deep/strict signatures
passed for all three placements. Launch Services `LSCanURLAcceptURL` returned
status 0 / accepts true for Viewer and All roles in all three placements.
This API result is not a Finder drag acceptance or launch result.

After normal Quit of the Store app, the maintainer repeated the icon-drop test
against this exact final read-only DMG: Finder displayed a prohibited cursor
and refused the drop. Earlier paired-candidate observations are therefore
reproduced against the actual selected final package. The cause remains
undetermined; signatures and Launch Services acceptance do not explain or
repair the Finder refusal.

### Maintainer-approved bounded release disposition

The maintainer explicitly approved the following disposition on October 23.
For v2.5.0, document PDF drop onto the application-bundle icon on a mounted
read-only DMG as a known unsupported route. Install the app in Applications
(or a writable user Applications folder) before using PDF drag and drop.
Use the installed app icon, its running Dock icon or presenter/slide windows;
all four routes were manually verified for both selected macOS channels.

The mounted-image app may still be used for the existing quick check through
Open / File > Open PDF; this disposition does not assert that every operation
on a mounted image fails. It accepts a specific known Finder-drop limitation,
not a test pass or a demonstrated OS root cause. It does not waive any customer
Store, hardware/OS, authority, bookmark, first-launch or security gate. No
package rebuild is needed for this documentation-only disposition.

## Evidence boundaries

Results only are committed. Raw logs, private account/settings data, test PDFs
and packages are preserved outside Git. Machine-readable follow-up results are
kept with the exact artifacts under ignored `artifacts/v250-final-20261023/`.
No application binary, signing, notarization or SBOM payload was changed.
Documentation-only changes require no Cargo rerun; `git diff --check` is the
applicable local check.

This report addresses the current-Mac document-delivery follow-up. Customer Store
coverage, unavailable hardware/OS, authority/bookmark and first-attempt-trace
dispositions, other platforms and pre-publication security review remain under
#508; this is not authorization to publish or close the whole release issue.
