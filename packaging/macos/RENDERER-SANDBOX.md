# macOS renderer privilege-separation decision (#374)

Status: implemented for Developer ID review. Runtime/security and lifetime gates
are exercised against real signed XPC code. Store signing configuration is a
candidate, not a validated Store release. The pre-release Developer ID
notarization/Gatekeeper rehearsal passed. Store packaging, sandboxed UI
and file-selection checks belong to #120 and do not block #374/#381. Do not close
#374 on unsigned CI or design documentation alone.

## Developer ID notarization rehearsal (2026-10-03)

The release executable from implementation commit `66f94f6` (head `76f3c18`
adds documentation only) was staged and signed with the nested XPC service and
PDFium. On macOS 26.7 / arm64, the existing `notarize_macos_dmg.sh` flow passed:

- Apple submission `e70f3917-4be3-4c70-96c5-8afd896df825`: `Accepted`,
  `Ready for distribution`, no issues in the notarization log.
- The ticket covers the outer app, proxy, native XPC service, both PDFium copies
  and the disk image. Stapling and ticket validation passed.
- DMG and mounted app Gatekeeper assessments: `accepted`,
  `source=Notarized Developer ID`.
- Strict nested signatures and mounted-DMG PDF open/render/notes passed.
- The mounted notarized payload also passed the actual XPC private-file read/write,
  network listen/connect and child-creation denial gate.
- Actual service signing entitlements contain only `com.apple.security.app-sandbox`.

Artifact: `QuickPresenter-1.5.0-pr381-rehearsal.dmg` (not published).
Post-stapling SHA-256:
`dd1af38394c6ce2a4cfe89d133e952eb0a10f69411430c3aeecc68c0e03795aa`.
Mach-O build UUID: `2F1A3460-05F0-3126-BB1F-687862AAF16F`.

This is evidence for the tested arm64 Developer ID candidate, not Store
certification or a full GUI/OS-version compatibility matrix. App Sandbox grants
required system/Mach and container access; no extra automation, device, app-group
or temporary-exception entitlements are supplied. There is no claim of blanket
UI/Mach denial. Validate the final v1.5.0 artifact before publication.

## App Sandbox and renderer isolation are complementary

Mac App Store submission requires App Sandbox. It is enabled by the signed
`com.apple.security.app-sandbox` entitlement, not by the distribution channel
automatically adding a policy. Developer ID signing, hardened runtime and
notarization alone do not enable App Sandbox. The DMG signing script now supplies
App Sandbox entitlements to the separate renderer, not to the whole UI.

App Sandbox restricts the whole application's resources. It does not mean that
every component can access only the PDF currently being rendered. The UI needs
preferences, recent-file state, file selection and presentation windows; native
PDF parsing must not gain those authorities merely by running in a child.

App Sandbox inheritance supplies the parent's static sandbox rights, not
PowerBox rights acquired after launch. Passing a selected PDF path to an
inheriting child is therefore neither reliable file authorization nor a separate
least-privilege boundary. Apple recommends XPC services for privilege separation.

## Decision: one supported renderer boundary for both distributions

Embed a separately signed XPC service, using its own minimal App Sandbox
entitlements, for both Developer ID DMG and Mac App Store builds. Do not stack
a hand-written Seatbelt profile on an existing App Sandbox. The former
`sandbox_init` experiment has been removed: the SDK marks that API deprecated
and no longer supported, and the prototype never demonstrated Store support.

| Target | Authority |
| --- | --- |
| Store UI | App Sandbox plus reviewed UI/file-selection permissions |
| DMG UI | Existing Developer ID policy; sandboxing the whole UI is a separate decision |
| XPC renderer, both builds | Own App Sandbox, no inheritance or network entitlements; only brokered PDF content and required runtime resources |

Do not grant the renderer home-directory access, user-selected-file/bookmark
entitlements, app groups/shared UI storage, devices, automation, or temporary
exceptions without a documented, tested requirement. App Sandbox does provide
container storage and system/runtime access: it is not a guarantee of zero
writes, zero Mach operations, or an exact syscall allowlist. Tests must establish
the precise denied UI/service/process operations claimed by the implementation.

## Required implementation and release gates

1. Move PDFium work into a nested `Contents/XPCServices/<service>.xpc`, with a distinct
   executable identity and entitlements. Authenticate the connection to the
   containing app and prevent unrelated clients from submitting PDF work.
2. The UI opens the selected regular file read-only under its own authorization.
   Transfer an owned read-only descriptor, or reviewed bounded bytes, through
   XPC. PDFium reads that object instead of reopening a display path. Retain
   preflight limits and safe process-global PDFium ownership; do not extend
   document lifetimes with unsafe conversions. Bookmarks for recent files belong
   to the UI, not the native renderer.
3. Preserve bounded/versioned requests and responses, session correlation,
   deadlines, restart limits, cancellation, memory monitoring and explicit
   termination on broker loss. XPC automatic lifecycle management alone is not
   evidence that these existing guarantees survive the transport change. Never
   trust a peer-supplied PID as authority to kill another process.
4. Stage the service and PDFium in reviewed bundle-relative locations. Sign
   nested libraries/service before the outer app with each target's own
   entitlements; verify those entitlements, strict signatures and library lookup.
   Validate the DMG through Gatekeeper and actual Developer ID notarization,
   and validate the Store variant separately under #120. Store readiness is not
   a completion gate for #374 or the direct-DMG implementation PR.
5. For each claimed package/version, test PDF open/render/notes plus denied
   unrelated private read/write, network listen/connect, and reviewed child,
   UI and Mach/service access. Include UI-owned settings/cache sentinels so a
   helper sharing UI authority cannot pass merely by denying arbitrary home
   files. Test broker crash, hung native work and concurrent active/candidate
   documents. Missing confinement must fail before PDF parsing; no unsandboxed
   release fallback or Store-support claim on unsigned developer evidence.

## Implemented transport and packaging

Each document starts `Contents/Helpers/RendererProxy.app` as a distinct client.
Its own `Contents/XPCServices/org.quickpresenter.renderer.xpc` is launched by
libXPC. The proxy is necessary because an application's XPC service is normally
shared: active and candidate documents need independent native processes.
The proxy never parses PDF bytes, creates UI, or initializes PDFium. It only
authenticates the service, transfers stdio and the read-only PDF FD, and supervises
the OS-reported service identity/memory. The existing bounded Rust pipe protocol
and broker watchdog remain unchanged above this bootstrap.

Both peers require an Apple-anchored signature with a fixed signing identifier
and their own running code's signing Team ID. They never copy requirements from
replaceable nested files or trust peer-supplied PIDs. No filesystem exception is
added so native code can inspect the UI/proxy signature. Untrusted/ad-hoc code
and unbundled release executables fail before PDF parsing; debug-only unbundled
helpers remain available for the existing protocol regression harness.

The service receives an owned, read-only regular-file descriptor, checks its
size/header, and uses PDFium's owned reader API under the existing process-global
`OnceLock` owner. The display path is not reopened. The descriptor is consumed
once: Close/Open cannot turn the service into a path-based reader. XPC loss and
the independent stdin guardian terminate it even during blocked native work.
`RLIMIT_NPROC` is hard-limited to zero to deny child creation independently of
App Sandbox; native threads remain usable. The proxy samples the native service's
resident memory against the existing 1 GiB budget, complementing UI deadlines.
As before, macOS resident-memory sampling is best effort, not a reservation cap.

`Renderer.entitlements` contains only `app-sandbox`. Developer ID proxies/UI are
not given extra sandbox permissions. `MACOS_DISTRIBUTION_MODE=app-store` supplies
exactly `app-sandbox`/`inherit` to the proxy and read-only user-selected access
to the sandboxed UI. This is a signing candidate only: actual Store identity,
PowerBox selection, recent-file/security-scoped persistence, UI behavior and
Store packaging/review remain unvalidated and are tracked in #120, not as blockers
for #374. No broad bookmark/app-group authority
is given to the renderer to compensate for missing UI integration.

Staging supplies ad-hoc layout signatures, not a runnable production security
claim. Developer ID signing signs PDFium and the XPC bundle first, then proxy,
then outer app; strict verification and the actual XPC denial gate run before
the signing script succeeds. CI with no trusted signing identity verifies
ad-hoc packages fail closed and records no native security success for them.

Run the signed release gate:

```sh
python3 scripts/check_macos_renderer.py "/path/Quick Presenter.app" tests/fixtures/marp-speaker-notes.pdf
```

With a separately signed **debug** bundle, add `--debug-tests` to test actual
service PIDs, deadlines, candidate isolation, one-shot recovery and broker loss.
Release builds exclude all fault/PID-report hooks in both Rust and the C shim.
Earlier custom Seatbelt test results are not evidence for this XPC boundary.

Developer ID notarization can be rehearsed before release using the existing
`scripts/notarize_macos_dmg.sh` procedure in `docs/PACKAGING.md`; neither a tag
nor a published release is required. Validate the final v1.5.0 artifact again
before publication. A rehearsal ticket does not establish notarization for a
subsequently changed binary or disk image.

## Sources

- [Configuring the macOS App Sandbox](https://developer.apple.com/documentation/xcode/configuring-the-macos-app-sandbox)
- [App Sandbox inheritance and entitlement rules](https://developer.apple.com/library/archive/documentation/Miscellaneous/Reference/EntitlementKeyReference/Chapters/EnablingAppSandbox.html)
- [XPC services and separate sandboxes](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingXPCServices.html)
- [User-selected files and security-scoped bookmarks](https://developer.apple.com/documentation/security/accessing-files-from-the-macos-app-sandbox)
