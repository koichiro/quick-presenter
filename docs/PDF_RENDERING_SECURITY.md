# PDF Rendering Security and Isolation Policy

## Purpose

Quick Presenter treats every PDF as untrusted input. PDFium is a native parser
and renderer, so Rust type safety, input preflight, and `catch_unwind()` do not
contain native crashes, hangs, memory corruption, or arbitrary code execution
inside PDFium or its dependencies.

This document defines the current release tradeoff, the target isolation model,
the failure contract that implementation work must preserve, and the PDFium
maintenance policy. It is a design and release policy; capabilities described as
targets are not security claims until their implementation and packaged tests
are complete.

## Trust model

The following inputs are untrusted:

- PDF bytes, metadata, page geometry, annotations, and speaker-note contents;
- file names and paths selected by a user or supplied on the command line;
- every message, length, identifier, pixel dimension, pixel buffer, error, and
  exit status received from a renderer helper;
- timing behavior, including a PDFium call that never returns.

The installed Quick Presenter executable, its packaged renderer helper, and the
pinned PDFium binary are trusted distribution artifacts. A renderer helper must
nevertheless be treated as compromised after it begins processing a PDF. The UI
process must not rely on helper-side validation to protect UI state or memory.

The target boundary is:

```text
Untrusted PDF
    |
    v
+---------------- sandboxed renderer helper ----------------+
| PDF preflight -> PDFium open/render/notes -> IPC response |
+-------------------------+----------------------------------+
                          | untrusted, versioned, bounded IPC
                          v
+---------------- privileged UI broker ----------------------+
| validate -> session acceptance -> cache -> Slint images   |
+------------------------------------------------------------+
```

The UI process is the broker. It owns presentation state, render scheduling,
session acceptance, caches, and Slint objects. The helper owns PDFium documents
and performs PDF parsing, page rendering, and note extraction. The helper must
not receive UI authority merely because the broker launched it.

## Isolation stages and guarantees

| Stage | Native crash or abort | Hard timeout | Compromised renderer authority |
| --- | --- | --- | --- |
| Previous render thread | Terminates the application | Not available while PDFium is running | Same user authority and address space as the UI |
| Current supervised, unsandboxed helper | Contained to the helper; bounded recovery | Separate operation deadlines, termination/reaping | Same user authority as the UI; **not a security boundary** |
| Supervised, OS-sandboxed helper | Contained to the helper | Broker can terminate and reap the helper | Restricted by the documented platform policy and brokered resources |

Process isolation is therefore the cross-platform reliability boundary.
Platform sandboxing is a separate defense-in-depth and least-privilege boundary.
Neither checksum pinning nor an unsandboxed child process makes a malicious PDF
safe.

## Current release tradeoff

Bundled macOS builds now use an independently App-Sandboxed XPC renderer with
brokered read-only document access and signature-authenticated peers. See
[macOS Renderer Sandbox](../packaging/macos/RENDERER-SANDBOX.md) for the validated
Developer ID runtime and notarization rehearsal evidence, and separate Store
requirements tracked in #120. Unsigned/ad-hoc
macOS release artifacts fail closed; they are not native security validation.

Other platforms on this branch keep `PdfDocumentState` in an unsandboxed helper, using
the same installed executable in internal helper mode. The UI/broker does not
initialize PDFium. Unrecoverable active-helper EOF, exit, or invalid IPC becomes `WorkerFailed`;
candidate failures keep the previous helper and last good slide. Shutdown kills
and reaps helpers without waiting for native work; parent-pipe EOF independently
exits the helper. This contains ordinary native crashes to the helper, but does
not prevent a compromised helper from exercising the user's OS authority.

Hard deadlines, bounded restart, and platform-specific memory controls are now
implemented. Their numeric values and fallback guarantees are documented in
[Render Scheduling](RENDER_SCHEDULING.md). macOS sampled RSS is not a hard memory
reservation cap. Packaged releases gain these guarantees only when they include
and validate this implementation; non-macOS helpers still have unsandboxed user authority
until their separate platform isolation PRs are incorporated.

Current mitigations reduce accidental and resource-exhaustion risk but do not
form a sandbox:

- the input must be a non-empty regular file beginning with `%PDF-`;
- input size is limited to 1 GiB before PDFium opens the file;
- render command and event backlogs are each bounded to 64 work items;
- rendered pages use the budgets in [Render Cache Budgets](CACHE_BUDGETS.md);
- packaged releases load a checksum-pinned bundled PDFium by default.

The IPC boundary enforces page-count, note-size, decoded-pixel, and dimension
caps documented in [Renderer IPC Protocol](RENDERER_PROTOCOL.md). Output caps do
not bound PDFium's internal allocations by themselves; platform memory controls
and deadlines provide the additional resource-exhaustion backstop described above.

## Target broker and helper contract

Issues #371 through #376 implement the target in separable layers:

- #371 defines versioned, bounded IPC and hostile-response validation.
- #372 moves production PDFium work into supervised helper processes.
- #373 adds deadlines, resource limits, fault injection, and recovery.
- #374, #375, and #376 add macOS, Windows, and Linux sandbox policies.

The common contract is:

1. The broker must validate protocol version, message kind, request and session
   identifiers, frame size, page count and index, pixel format, dimensions,
   checked `width * height * 4`, note size, and error size before allocation or
   presentation-state mutation.
2. Protocol traffic uses bounded framing. Raw RGBA bytes may follow a validated
   control header; they must not be expanded into textual arrays or base64.
3. Unknown versions, malformed or truncated frames, impossible dimensions,
   oversized payloads, stale responses, and unsolicited responses are protocol
   failures. The broker discards the response, terminates and reaps that helper,
   records a bounded diagnostic, and does not retry the rejected command.
4. Every PDFium operation has a named deadline appropriate to open, visible
   render, auxiliary render, note extraction, or graceful shutdown. When a hard
   deadline expires, the broker terminates and reaps the helper; it never tries
   to cancel a thread inside PDFium.
5. Input bytes, page count, target dimensions, decoded pixels and bytes, IPC
   frames, note text, helper memory, queued work, and diagnostic text all have
   named limits. Checked arithmetic and broker-side limits apply before broker
   allocation; helper preflight checks output geometry before native bitmap
   allocation. Platform memory caps have the documented macOS sampled fallback.
6. A helper cannot outlive its broker during normal shutdown or supported
   packaged-process termination. Platform lifecycle controls must cover any
   process tree the helper could create.

## Presentation recovery policy

Replacement opens and hot reloads must be prepared by a candidate helper. The
active helper and committed presentation remain unchanged until the candidate
has opened the PDF and returned a valid initial current-page render. Candidate
crash, timeout, protocol failure, or ordinary PDF error discards only the
candidate and preserves the committed deck.

If an active helper fails, the broker must:

1. invalidate requests and responses for the failed helper session;
2. keep the last-good current audience image visible and retain safe broker-side
   cached pixels;
3. show a short presenter-facing recovery message without exposing PDF content
   or sensitive paths;
4. for a crash/EOF/timeout, automatically attempt at most one helper restart for the same document in a
   rolling 60-second window, reopening the document and rerendering the current
   page; and
5. suppress further automatic restarts after another failure until the user
   explicitly opens or retries a document.

A protocol-version mismatch is a packaging error and is never automatically
downgraded. Malformed IPC is not retried. An explicit user action starts a new
session and may attempt recovery after automatic restart suppression.

Diagnostics may contain operation names, bounded identifiers, PDFium and
protocol versions, process exit classification, and applied limit names. Native
failure/protocol diagnostics omit helper-controlled error chains. They must not
contain PDF bytes, rendered pixels, speaker notes, full
file dumps, or unnecessary full paths.

## Platform sandbox policy

The platform issues must document exactly which packaged builds provide a
security boundary. An unsupported platform or packaging mode must fail closed
when that is practical, or be described explicitly as unsandboxed crash
containment. It must never silently receive the same security claim as a tested
sandboxed package.

- **macOS (#374):** use an embedded XPC service with its own minimal App Sandbox
  and read-only brokered PDF descriptor. Document-specific proxy clients retain
  active/candidate native-process independence. App Sandbox inheritance is used
  only by the Store proxy, never as the renderer's privilege-separation boundary.
- **Windows (#375):** use AppContainer or LPAC for authority reduction and a Job
  Object for process-tree lifetime and resource limits.
- **Linux (#376):** for direct packages, use `no_new_privs`, inherited-descriptor
  cleanup, Landlock where supported, and a measured seccomp policy. Flatpak
  builds use narrow permissions and the document portal. Namespace isolation is
  defense in depth, not the filesystem access-control policy.

Each sandbox must deny network access, unrelated user files, UI control, and
child-process creation unless a documented and tested PDFium requirement needs
a narrower exception. Packaged tests must prove both successful PDF operation
and denial of representative unrelated resources.

## PDFium version and vulnerability response

Quick Presenter supports the single PDFium build pinned by
`scripts/pdfium_manifest.json`. Packaged releases do not support arbitrary
system PDFium versions. The guarded development override is a troubleshooting
facility, not a supported production configuration.

The release maintainer follows this owner-neutral schedule:

- review new `bblanchon/pdfium-binaries` releases and relevant PDFium/Chromium
  security information at least monthly and before every Quick Presenter
  release;
- adopt a validated current PDFium build at least once per Quick Presenter
  minor release or once per quarter, whichever comes first, unless a tracked
  compatibility blocker records why the older pin remains necessary;
- record the reviewed current pin, latest available build, relevant changes,
  and decision in the update issue or release checklist.

Use these primary monitoring inputs:

- the [prebuilt PDFium release feed](https://github.com/bblanchon/pdfium-binaries/releases)
  used by the pinned manifest;
- the [upstream PDFium commit log](https://pdfium.googlesource.com/pdfium/+log/refs/heads/main);
  and
- the [Chromium PDFium security page](https://www.chromium.org/Home/chromium-security/pdfium-security/)
  and Chromium security announcements.

For a disclosed issue that can affect parsing, rendering, fonts, images,
annotations, or other enabled PDFium functionality:

1. Open or update a security-sensitive tracking item and complete initial
   relevance triage within two business days. Keep embargoed details in a
   private security advisory or equivalent private record until disclosure is
   appropriate.
2. If an upstream/prebuilt fix is available, prepare and validate a candidate
   pin within seven calendar days. Act sooner for known exploitation or a
   severity that threatens arbitrary code execution.
3. Verify upstream tag and change provenance, release digests, manifest changes,
   extracted `VERSION`, licenses, fetch tests, Rust checks, compatibility
   fixtures, and supported-platform packaged smoke checks.
4. Publish an expedited Quick Presenter update as soon as required platform
   validation passes. Do not wait for the normal quarterly adoption point.
5. If no usable fix exists, document affected versions and mitigations, avoid a
   new release that overstates safety, and disable or withdraw an affected
   distribution when the risk cannot be acceptably mitigated.

Checksum verification protects artifact selection and transport; it does not
establish that a PDFium build has no vulnerabilities. Security fixes use the
same pinned-manifest workflow documented in [Packaging Notes](PACKAGING.md).

## Release claim checklist

Before a release claims helper isolation or sandbox protection, confirm that:

- production open, render, thumbnail, preload, hot-reload, and note paths do not
  initialize PDFium in the UI process;
- abnormal exit, abort, hang, malformed IPC, oversized response, and repeated
  failure tests pass;
- the package-specific sandbox denies representative unrelated-file and network
  access while normal rendering succeeds;
- the helper and PDFium lookup, signing, licenses, versions, and process cleanup
  pass packaged smoke checks; and
- the PDFium review record satisfies the cadence and vulnerability policy above.

Until all applicable checks pass, release notes must describe the implemented
stage accurately rather than using the word “sandbox” for thread confinement or
an unsandboxed child process.
