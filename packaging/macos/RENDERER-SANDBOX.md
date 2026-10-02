# Experimental Developer ID renderer confinement (#374)

This branch is a **draft, not a supported production sandbox or Mac App Store
implementation**. It evaluates a deny-default Seatbelt profile before any PDF
parsing. The macOS SDK marks `sandbox_init` deprecated and "No longer supported";
custom profiles are not a supported App Sandbox entitlement interface. Do not
close #374 or ship this implementation as its final security boundary.

The profile grants read access to the canonical selected PDF, system libraries
and system fonts. Trusted bundled PDFium initialization occurs before confinement
but does not parse a PDF. No home-directory grant, network, process execution,
UI/Mach lookup, or filesystem write grant is supplied. The original path is
canonicalized before confinement; paths containing controls are rejected and
profile metacharacters are escaped. Failure to install the policy exits before
PDF work; there is no unsandboxed downgrade. Debug-only fault markers have an
exact-file exception and are absent from release builds.

## Shipping design decision

Apple recommends an embedded XPC service with its own App Sandbox entitlement
boundary. Merely adding `app-sandbox`/`inherit` to the self-spawned executable
does not transfer PowerBox access and is not separate privilege separation.
The final implementation should move native work to a separately signed XPC
service, transfer read-only file descriptors or bounded brokered bytes, and
retain the existing versioned validation, deadlines, and termination semantics.
No bookmark or broad user-selected-file entitlement should be given to native
code unless reviewed and demonstrated necessary.

DMG hardened-runtime signing remains unchanged for this experiment: there are no
new sandbox, network, disable-library-validation, or JIT entitlements and no
nested code to sign. This does **not** validate the final XPC nesting/signing
order. Mac App Store is unsupported; an inherited App Sandbox that prevents
installing this policy fails closed.

## Experimental gate

Build release, stage the app, then run:

```sh
python3 scripts/check_renderer_sandbox.py "/path/Quick Presenter.app/Contents/MacOS/quick-presenter" tests/fixtures/marp-speaker-notes.pdf
```

The gate uses a broker-readable private sentinel and a broker-owned loopback
listener: the renderer must reject reads/writes/listen/connect/child execution
while PDF open/render/notes succeed. Strict codesign, Gatekeeper and actual
Developer ID notarization remain release gates, not claims established by an
unsigned development run. XPC-based packaged denial tests are still required.

Reference: [Apple sandbox and XPC guidance](https://developer.apple.com/library/archive/documentation/Miscellaneous/Reference/EntitlementKeyReference/Chapters/EnablingAppSandbox.html).
