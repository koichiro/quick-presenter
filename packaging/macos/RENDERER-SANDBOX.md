# macOS renderer privilege-separation decision (#374)

Status: design only. The XPC transport, separate signed service, entitlements,
and packaged security gates are not implemented by this PR. The existing
self-spawned renderer still provides crash containment, not a supported macOS
least-privilege boundary. Do not close #374 on this document's evidence.

## App Sandbox and renderer isolation are complementary

Mac App Store submission requires App Sandbox. It is enabled by the signed
`com.apple.security.app-sandbox` entitlement, not by the distribution channel
automatically adding a policy. Developer ID signing, hardened runtime and
notarization alone do not enable App Sandbox. The current DMG signing script
does not supply App Sandbox entitlements.

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

1. Move PDFium work into `Contents/XPCServices/<service>.xpc`, with a distinct
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
   and the Store variant through its own signing/packaging validation.
5. For each claimed package/version, test PDF open/render/notes plus denied
   unrelated private read/write, network listen/connect, and reviewed child,
   UI and Mach/service access. Include UI-owned settings/cache sentinels so a
   helper sharing UI authority cannot pass merely by denying arbitrary home
   files. Test broker crash, hung native work and concurrent active/candidate
   documents. Missing confinement must fail before PDF parsing; no unsandboxed
   release fallback or Store-support claim on unsigned developer evidence.

The earlier experimental denial results do not validate this XPC design. The
existing generic denial script on other platform branches may be reused only
after the macOS service performs probes inside the actual sandboxed renderer.

## Sources

- [Configuring the macOS App Sandbox](https://developer.apple.com/documentation/xcode/configuring-the-macos-app-sandbox)
- [App Sandbox inheritance and entitlement rules](https://developer.apple.com/library/archive/documentation/Miscellaneous/Reference/EntitlementKeyReference/Chapters/EnablingAppSandbox.html)
- [XPC services and separate sandboxes](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingXPCServices.html)
- [User-selected files and security-scoped bookmarks](https://developer.apple.com/documentation/security/accessing-files-from-the-macos-app-sandbox)
