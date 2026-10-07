# Changelog

## [1.0.2] — development checkpoint, 2026-10-02

No GitHub release or tag was published for this version. [#368](https://github.com/koichiro/quick-presenter/pull/368) advanced package
metadata after v1.0.1; the following preparation work landed while the package
reported 1.0.2.

### Development

- Document macOS signing/notarization credential preflight and artifact handoff
  requirements ([#369](https://github.com/koichiro/quick-presenter/pull/369)).
- Define PDF renderer isolation, threat-model, and vulnerability-response policy
  ([#241](https://github.com/koichiro/quick-presenter/issues/241), [#370](https://github.com/koichiro/quick-presenter/issues/370), [#377](https://github.com/koichiro/quick-presenter/pull/377)).
- Add a bounded, versioned renderer IPC codec and broker adapter that reject
  malformed, stale, unsolicited, and oversized responses ([#371](https://github.com/koichiro/quick-presenter/issues/371), [#378](https://github.com/koichiro/quick-presenter/pull/378)).

## [1.0.1] — 2026-10-02

### Fixed

- Remove the 1024 × 576 audience-window cap that prevented slides from filling
  native macOS fullscreen. Preserve normal windowed preferred/minimum sizing.
  Windows and Linux were unaffected ([#366](https://github.com/koichiro/quick-presenter/issues/366), [#367](https://github.com/koichiro/quick-presenter/pull/367)).

## [1.0.0] — 2026-10-02

First stable release, following the internal 0.x series.

### Added

- Automatically reload the active PDF after external saves or atomic replacement.
  Preserve the current page where possible and keep the last good deck visible
  through incomplete saves or failed reloads ([#347](https://github.com/koichiro/quick-presenter/issues/347), [#351](https://github.com/koichiro/quick-presenter/pull/351), [#352](https://github.com/koichiro/quick-presenter/pull/352)).
- Validate representative PDF exports from Keynote, PowerPoint, Google Slides,
  Marp, and LaTeX Beamer, and document the Beamer `pdfcomment` speaker-note
  workflow ([#278](https://github.com/koichiro/quick-presenter/issues/278), [#338](https://github.com/koichiro/quick-presenter/pull/338)).

### Changed

- Clearly reject password-protected PDFs, without adding a password prompt or
  interrupting an already active presentation ([#341](https://github.com/koichiro/quick-presenter/issues/341), [#354](https://github.com/koichiro/quick-presenter/pull/354)).
- Bound diagnostic log size and retention and enforce private permissions where
  supported ([#340](https://github.com/koichiro/quick-presenter/issues/340), [#353](https://github.com/koichiro/quick-presenter/pull/353), [#355](https://github.com/koichiro/quick-presenter/pull/355)).
- Upgrade Slint to 1.18.1 and bundled PDFium to `chromium/8076`
  ([#357](https://github.com/koichiro/quick-presenter/issues/357), [#358](https://github.com/koichiro/quick-presenter/pull/358), [#359](https://github.com/koichiro/quick-presenter/issues/359), [#361](https://github.com/koichiro/quick-presenter/pull/361)).
- Resolve the tracked `quick-xml` advisories through upstream dependency updates
  and remove temporary audit exceptions ([#329](https://github.com/koichiro/quick-presenter/issues/329), [#349](https://github.com/koichiro/quick-presenter/pull/349)).
- Publish the production website and define supported distribution: signed and
  notarized macOS DMG, Ubuntu x64 `.deb`, and Microsoft Store-only Windows
  installation. Windows Store update verification was deferred at GitHub release
  publication; the existing Store version then was 0.7.2
  ([#276](https://github.com/koichiro/quick-presenter/issues/276), [#280](https://github.com/koichiro/quick-presenter/issues/280), [#288](https://github.com/koichiro/quick-presenter/issues/288), [#362](https://github.com/koichiro/quick-presenter/pull/362), [#363](https://github.com/koichiro/quick-presenter/pull/363), [#364](https://github.com/koichiro/quick-presenter/pull/364)).
- Change the license from GPL-3.0-or-later to GPL-3.0-only ([#337](https://github.com/koichiro/quick-presenter/pull/337)).

## 0.7.2 — Microsoft Store channel, publication date not recorded here

No `v0.7.2` Git tag or GitHub release exists. Package metadata was advanced on
2026-07-09, and the v1.0.0 release notes confirm that 0.7.2 was available through
Microsoft Store. Store-readiness work below landed during this development
cycle; the absence of a Store source tag prevents an exact shipped commit range.

### Distribution

- Add a Microsoft Store-identity MSIX build/verification path and document the
  package identity and submission workflow ([#111](https://github.com/koichiro/quick-presenter/issues/111), [#286](https://github.com/koichiro/quick-presenter/pull/286)).
- Prepare Store listing icons, Windows screenshots, and submission-oriented
  website/privacy pages ([#334](https://github.com/koichiro/quick-presenter/pull/334), [#335](https://github.com/koichiro/quick-presenter/pull/335), [#336](https://github.com/koichiro/quick-presenter/pull/336), [#343](https://github.com/koichiro/quick-presenter/pull/343)).

Other changes made while Cargo still reported 0.7.2 are listed under v1.0.0,
where a tagged release establishes their inclusion.

## [0.7.1] — 2026-07-08

Internal release focused on Linux window/menu behavior and packaging.

### Fixed

- Restore readable, functional Linux menus with an inline presenter menu and
  improve Wayland startup layout/window sizing ([#299](https://github.com/koichiro/quick-presenter/issues/299), [#332](https://github.com/koichiro/quick-presenter/pull/332)).
- Tighten presenter notes/status spacing and empty next-preview layout, and
  constrain the audience window to the fitted slide size ([#324](https://github.com/koichiro/quick-presenter/issues/324), [#325](https://github.com/koichiro/quick-presenter/pull/325), [#332](https://github.com/koichiro/quick-presenter/pull/332)).
- Correct PDFium version metadata shown in About ([#320](https://github.com/koichiro/quick-presenter/issues/320), [#321](https://github.com/koichiro/quick-presenter/pull/321)).

### Changed

- Update bundled PDFium to `chromium/7920` ([#322](https://github.com/koichiro/quick-presenter/issues/322), [#323](https://github.com/koichiro/quick-presenter/pull/323)).
- Normalize Debian package file modes and document Linux PDF MIME handler policy
  ([#326](https://github.com/koichiro/quick-presenter/issues/326), [#327](https://github.com/koichiro/quick-presenter/issues/327), [#330](https://github.com/koichiro/quick-presenter/pull/330), [#331](https://github.com/koichiro/quick-presenter/pull/331)).
- Harden macOS signing checks. The release accepted the tracked `quick-xml`
  advisories for this internal version; upstream fixes landed for v1.0.0 ([#329](https://github.com/koichiro/quick-presenter/issues/329)).

## [0.7.0] — 2026-06-30

Internal release focused on presenter polish and packaging readiness.

### Added

- Show slide progress in the presenter window ([#293](https://github.com/koichiro/quick-presenter/issues/293), [#315](https://github.com/koichiro/quick-presenter/pull/315)).
- Make long speaker notes scrollable using measured text height ([#255](https://github.com/koichiro/quick-presenter/issues/255), [#261](https://github.com/koichiro/quick-presenter/pull/261)).
- Preflight PDF inputs and reject missing, empty, non-PDF, directory, and oversized
  inputs before parsing; show status during slow opens ([#253](https://github.com/koichiro/quick-presenter/issues/253), [#254](https://github.com/koichiro/quick-presenter/issues/254), [#264](https://github.com/koichiro/quick-presenter/pull/264), [#275](https://github.com/koichiro/quick-presenter/pull/275)).
- Add a discoverable diagnostic log path, owner-private recent-file storage,
  project website, and privacy policy ([#277](https://github.com/koichiro/quick-presenter/issues/277), [#279](https://github.com/koichiro/quick-presenter/issues/279), [#287](https://github.com/koichiro/quick-presenter/pull/287), [#289](https://github.com/koichiro/quick-presenter/issues/289), [#290](https://github.com/koichiro/quick-presenter/pull/290), [#316](https://github.com/koichiro/quick-presenter/pull/316), [#317](https://github.com/koichiro/quick-presenter/pull/317)).

### Fixed

- Preserve a resized audience window during navigation and correct slide aspect
  sizing/margins ([#273](https://github.com/koichiro/quick-presenter/issues/273), [#274](https://github.com/koichiro/quick-presenter/pull/274), [#294](https://github.com/koichiro/quick-presenter/issues/294), [#295](https://github.com/koichiro/quick-presenter/pull/295)).
- Improve Linux launcher startup, desktop app identity, asynchronous file dialogs,
  and keyboard focus recovery after opening PDFs
  ([#221](https://github.com/koichiro/quick-presenter/issues/221), [#291](https://github.com/koichiro/quick-presenter/pull/291), [#296](https://github.com/koichiro/quick-presenter/issues/296), [#297](https://github.com/koichiro/quick-presenter/issues/297), [#298](https://github.com/koichiro/quick-presenter/pull/298), [#300](https://github.com/koichiro/quick-presenter/pull/300), [#302](https://github.com/koichiro/quick-presenter/issues/302), [#303](https://github.com/koichiro/quick-presenter/pull/303), [#307](https://github.com/koichiro/quick-presenter/pull/307)).
- Fix thumbnail clipping when no scrollbar is needed and reduce oversized macOS
  app-switcher icons ([#284](https://github.com/koichiro/quick-presenter/issues/284), [#285](https://github.com/koichiro/quick-presenter/pull/285), [#301](https://github.com/koichiro/quick-presenter/issues/301), [#313](https://github.com/koichiro/quick-presenter/pull/313)).

### Development

- Upgrade Slint to 1.17 and `rfd` to 0.17. Pin `pdfium-render` back to 0.9.1 for
  Linux compiler compatibility after evaluating 0.9.2 ([#292](https://github.com/koichiro/quick-presenter/issues/292), [#308](https://github.com/koichiro/quick-presenter/pull/308), [#314](https://github.com/koichiro/quick-presenter/pull/314)).
- Add scheduled dependency checks, optional Linux cross checks, cache-budget
  measurements, and recorded manual GUI/focus validation scenarios
  ([#256](https://github.com/koichiro/quick-presenter/issues/256), [#257](https://github.com/koichiro/quick-presenter/issues/257), [#258](https://github.com/koichiro/quick-presenter/issues/258), [#259](https://github.com/koichiro/quick-presenter/issues/259), [#262](https://github.com/koichiro/quick-presenter/pull/262), [#263](https://github.com/koichiro/quick-presenter/pull/263), [#266](https://github.com/koichiro/quick-presenter/pull/266), [#272](https://github.com/koichiro/quick-presenter/pull/272), [#303](https://github.com/koichiro/quick-presenter/pull/303)).

## [0.6.0] — 2026-06-23

Internal reliability release.

### Fixed

- Surface render worker failures and recreate the scheduler after recoverable
  failures; consolidate scheduling and document worker lifecycle
  ([#210](https://github.com/koichiro/quick-presenter/issues/210), [#217](https://github.com/koichiro/quick-presenter/pull/217), [#232](https://github.com/koichiro/quick-presenter/issues/232), [#239](https://github.com/koichiro/quick-presenter/pull/239), [#240](https://github.com/koichiro/quick-presenter/issues/240), [#242](https://github.com/koichiro/quick-presenter/issues/242), [#245](https://github.com/koichiro/quick-presenter/pull/245), [#249](https://github.com/koichiro/quick-presenter/pull/249)).
- Prioritize visible renders over speaker-note extraction ([#211](https://github.com/koichiro/quick-presenter/issues/211), [#226](https://github.com/koichiro/quick-presenter/pull/226)).
- Render thumbnails for the scrolled viewport so later pages remain reachable
  ([#222](https://github.com/koichiro/quick-presenter/issues/222), [#223](https://github.com/koichiro/quick-presenter/pull/223)).
- Keep the current deck visible while opening a replacement and when that open
  fails ([#231](https://github.com/koichiro/quick-presenter/issues/231), [#237](https://github.com/koichiro/quick-presenter/pull/237)).
- Make presenter window hiding/closing recoverable, fix macOS slide titlebar
  clipping, reduce Windows titlebar contrast, and fix Linux PDF file-association
  startup ([#128](https://github.com/koichiro/quick-presenter/issues/128), [#206](https://github.com/koichiro/quick-presenter/pull/206), [#224](https://github.com/koichiro/quick-presenter/issues/224), [#225](https://github.com/koichiro/quick-presenter/pull/225), [#229](https://github.com/koichiro/quick-presenter/issues/229), [#230](https://github.com/koichiro/quick-presenter/issues/230), [#235](https://github.com/koichiro/quick-presenter/pull/235), [#236](https://github.com/koichiro/quick-presenter/pull/236)).

### Development

- Rename the GUI executable to `quick-presenter`; reserve `qp` for a future
  automation CLI ([#218](https://github.com/koichiro/quick-presenter/issues/218), [#219](https://github.com/koichiro/quick-presenter/pull/219)).
- Add GUI smoke validation, exercise asynchronous rendering, run Linux GUI smoke
  in package builds, and add dependency advisory auditing
  ([#208](https://github.com/koichiro/quick-presenter/issues/208), [#212](https://github.com/koichiro/quick-presenter/issues/212), [#215](https://github.com/koichiro/quick-presenter/pull/215), [#228](https://github.com/koichiro/quick-presenter/pull/228), [#233](https://github.com/koichiro/quick-presenter/issues/233), [#234](https://github.com/koichiro/quick-presenter/issues/234), [#248](https://github.com/koichiro/quick-presenter/pull/248), [#250](https://github.com/koichiro/quick-presenter/pull/250)).

## [0.5.1] — 2026-06-21

Internal stability release.

### Fixed

- Stabilize repeated native macOS fullscreen transitions and boundary/key-repeat
  navigation; avoid unnecessary refreshes when the page does not change
  ([#196](https://github.com/koichiro/quick-presenter/issues/196), [#198](https://github.com/koichiro/quick-presenter/pull/198)).
- Restore macOS Ctrl+A / Ctrl+E first/last slide shortcuts ([#197](https://github.com/koichiro/quick-presenter/issues/197), [#203](https://github.com/koichiro/quick-presenter/pull/203)).
- Hide the extra Windows console in release builds ([#193](https://github.com/koichiro/quick-presenter/issues/193), [#195](https://github.com/koichiro/quick-presenter/pull/195)).

### Distribution

- Use versioned macOS/Windows package filenames, harden signed/notarized DMG
  packaging and mounted-image checks, and correct stale artifact instructions
  ([#189](https://github.com/koichiro/quick-presenter/issues/189), [#190](https://github.com/koichiro/quick-presenter/pull/190), [#191](https://github.com/koichiro/quick-presenter/issues/191), [#192](https://github.com/koichiro/quick-presenter/pull/192), [#194](https://github.com/koichiro/quick-presenter/issues/194), [#204](https://github.com/koichiro/quick-presenter/pull/204)).

## [0.5.0] — 2026-06-20

Internal release focused on rendering/runtime reliability and package validation.

### Changed

- Move rendering off the UI thread into a worker with cancellation boundaries and
  bounded scheduler backpressure ([#151](https://github.com/koichiro/quick-presenter/issues/151), [#162](https://github.com/koichiro/quick-presenter/pull/162), [#170](https://github.com/koichiro/quick-presenter/issues/170), [#173](https://github.com/koichiro/quick-presenter/issues/173), [#179](https://github.com/koichiro/quick-presenter/pull/179), [#180](https://github.com/koichiro/quick-presenter/pull/180)).
- Commit replacement PDF sessions only after the initial render succeeds and keep
  the last good audience image on current-page cache misses or failures
  ([#154](https://github.com/koichiro/quick-presenter/issues/154), [#160](https://github.com/koichiro/quick-presenter/pull/160), [#171](https://github.com/koichiro/quick-presenter/issues/171), [#177](https://github.com/koichiro/quick-presenter/pull/177)).
- Bound render-cache memory, handle oversized visible pages, lazy-load thumbnails,
  and window thumbnail state for large decks ([#153](https://github.com/koichiro/quick-presenter/issues/153), [#161](https://github.com/koichiro/quick-presenter/pull/161), [#172](https://github.com/koichiro/quick-presenter/issues/172), [#174](https://github.com/koichiro/quick-presenter/issues/174), [#182](https://github.com/koichiro/quick-presenter/pull/182), [#183](https://github.com/koichiro/quick-presenter/pull/183)).
- Split application controllers and clarify PDFium lifetime/worker ownership
  ([#132](https://github.com/koichiro/quick-presenter/issues/132), [#146](https://github.com/koichiro/quick-presenter/pull/146), [#155](https://github.com/koichiro/quick-presenter/issues/155), [#163](https://github.com/koichiro/quick-presenter/pull/163), [#175](https://github.com/koichiro/quick-presenter/issues/175), [#184](https://github.com/koichiro/quick-presenter/pull/184)).
- Harden PDFium lookup/download verification and recent-file privacy/storage
  ([#152](https://github.com/koichiro/quick-presenter/issues/152), [#156](https://github.com/koichiro/quick-presenter/issues/156), [#164](https://github.com/koichiro/quick-presenter/pull/164), [#165](https://github.com/koichiro/quick-presenter/pull/165)).

### Added

- Add ergonomic first/last slide shortcuts and CLI `--help` ([#144](https://github.com/koichiro/quick-presenter/issues/144), [#145](https://github.com/koichiro/quick-presenter/pull/145), [#150](https://github.com/koichiro/quick-presenter/issues/150), [#166](https://github.com/koichiro/quick-presenter/pull/166)).
- Add Ubuntu `.deb` packaging, Windows MSIX layout validation, optional Windows
  signing, and a pinned WiX 5.0.2 build ([#135](https://github.com/koichiro/quick-presenter/issues/135), [#137](https://github.com/koichiro/quick-presenter/pull/137), [#138](https://github.com/koichiro/quick-presenter/pull/138), [#141](https://github.com/koichiro/quick-presenter/pull/141), [#142](https://github.com/koichiro/quick-presenter/pull/142)).
- Document packaged usage, authoring-tool expectations, and clean PDFium
  extraction; bundle PDFium `151.0.7891.0` in release artifacts
  ([#143](https://github.com/koichiro/quick-presenter/pull/143), [#149](https://github.com/koichiro/quick-presenter/issues/149), [#167](https://github.com/koichiro/quick-presenter/pull/167), [#176](https://github.com/koichiro/quick-presenter/issues/176), [#186](https://github.com/koichiro/quick-presenter/pull/186), [#188](https://github.com/koichiro/quick-presenter/pull/188)).

The v0.5.0 release notes originally used the 0.4.0 version merge commit as their
comparison base because the `v0.4.0` tag did not yet exist. That tag now points to
the same checkpoint.

## [0.4.0] — tagged checkpoint, 2026-06-19

No GitHub release record exists for this version.

### Fixed

- Reset the elapsed timer when returning to the first page and close the audience
  window when the presenter closes ([#123](https://github.com/koichiro/quick-presenter/issues/123), [#124](https://github.com/koichiro/quick-presenter/pull/124), [#125](https://github.com/koichiro/quick-presenter/issues/125), [#127](https://github.com/koichiro/quick-presenter/pull/127)).
- Make the macOS audience titlebar less intrusive ([#56](https://github.com/koichiro/quick-presenter/issues/56), [#130](https://github.com/koichiro/quick-presenter/pull/130)).

### Distribution

- Add Developer ID signing, notarization, and Gatekeeper assessment scripts for
  macOS distribution ([#118](https://github.com/koichiro/quick-presenter/issues/118), [#119](https://github.com/koichiro/quick-presenter/pull/119)).
- Change the project license from Apache-2.0 to GPL-3.0-or-later ([#121](https://github.com/koichiro/quick-presenter/issues/121), [#122](https://github.com/koichiro/quick-presenter/pull/122)).

## [0.3.0] — initial MVP tagged checkpoint, 2026-06-12

No GitHub release record exists for this version. This checkpoint includes the
initial playback implementation developed before the 0.3.0 version bump.

### Added

- Open PDF slide decks from the UI or `--pdf`, with presenter-facing errors,
  document/page status, and recent-file storage/menu
  ([#26](https://github.com/koichiro/quick-presenter/issues/26), [#28](https://github.com/koichiro/quick-presenter/issues/28), [#37](https://github.com/koichiro/quick-presenter/pull/37), [#52](https://github.com/koichiro/quick-presenter/issues/52), [#55](https://github.com/koichiro/quick-presenter/pull/55), [#58](https://github.com/koichiro/quick-presenter/pull/58), [#61](https://github.com/koichiro/quick-presenter/pull/61)).
- Synchronize presenter and audience windows with current/next slide previews,
  PDF annotation speaker notes, elapsed timer, and clock
  ([#1](https://github.com/koichiro/quick-presenter/issues/1), [#2](https://github.com/koichiro/quick-presenter/issues/2), [#3](https://github.com/koichiro/quick-presenter/issues/3), [#4](https://github.com/koichiro/quick-presenter/issues/4), [#5](https://github.com/koichiro/quick-presenter/issues/5), [#20](https://github.com/koichiro/quick-presenter/issues/20), [#21](https://github.com/koichiro/quick-presenter/issues/21), [#22](https://github.com/koichiro/quick-presenter/issues/22)).
- Add keyboard/remote navigation, first/last page jumps, fullscreen, black screen,
  and a visible keyboard reference ([#6](https://github.com/koichiro/quick-presenter/issues/6), [#8](https://github.com/koichiro/quick-presenter/issues/8), [#24](https://github.com/koichiro/quick-presenter/issues/24), [#27](https://github.com/koichiro/quick-presenter/issues/27), [#30](https://github.com/koichiro/quick-presenter/issues/30), [#93](https://github.com/koichiro/quick-presenter/pull/93), [#103](https://github.com/koichiro/quick-presenter/issues/103), [#105](https://github.com/koichiro/quick-presenter/pull/105)).
- Add aspect-aware slide sizing, render caching/preload, and scrollable thumbnail
  navigation ([#25](https://github.com/koichiro/quick-presenter/issues/25), [#39](https://github.com/koichiro/quick-presenter/issues/39), [#40](https://github.com/koichiro/quick-presenter/pull/40), [#41](https://github.com/koichiro/quick-presenter/pull/41), [#78](https://github.com/koichiro/quick-presenter/issues/78), [#98](https://github.com/koichiro/quick-presenter/pull/98), [#115](https://github.com/koichiro/quick-presenter/pull/115)).
- Add About/version/license metadata, cross-platform icons, bundled PDFium lookup,
  macOS app/DMG and Windows MSI packaging, CI binary builds, and packaged smoke
  checks ([#48](https://github.com/koichiro/quick-presenter/issues/48), [#53](https://github.com/koichiro/quick-presenter/issues/53), [#69](https://github.com/koichiro/quick-presenter/issues/69), [#71](https://github.com/koichiro/quick-presenter/issues/71), [#86](https://github.com/koichiro/quick-presenter/issues/86), [#87](https://github.com/koichiro/quick-presenter/issues/87), [#88](https://github.com/koichiro/quick-presenter/issues/88), [#94](https://github.com/koichiro/quick-presenter/issues/94), [#101](https://github.com/koichiro/quick-presenter/issues/101)).

[1.0.2]: https://github.com/koichiro/quick-presenter/compare/v1.0.1...3e06410
[1.0.1]: https://github.com/koichiro/quick-presenter/releases/tag/v1.0.1
[1.0.0]: https://github.com/koichiro/quick-presenter/releases/tag/v1.0.0
[0.7.1]: https://github.com/koichiro/quick-presenter/releases/tag/v0.7.1
[0.7.0]: https://github.com/koichiro/quick-presenter/releases/tag/v0.7.0
[0.6.0]: https://github.com/koichiro/quick-presenter/releases/tag/v0.6.0
[0.5.1]: https://github.com/koichiro/quick-presenter/releases/tag/v0.5.1
[0.5.0]: https://github.com/koichiro/quick-presenter/releases/tag/v0.5.0
[0.4.0]: https://github.com/koichiro/quick-presenter/tree/v0.4.0
[0.3.0]: https://github.com/koichiro/quick-presenter/tree/v0.3.0
