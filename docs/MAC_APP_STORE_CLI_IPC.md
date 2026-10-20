# Mac App Store CLI control design

Tracks [#431](https://github.com/koichiro/quick-presenter/issues/431) under
[#422](https://github.com/koichiro/quick-presenter/issues/422). This is the
architecture for the optional v2.5.0 Store control build, not yet a qualified
customer Store feature.
The Developer ID macOS and Linux distributions retain their Unix socket; Windows
retains its named pipe. The public `qp` commands and Control Protocol v1 remain
the compatibility contract.

## Decision and evidence

Use a separate, per-user XPC broker for Store CLI rendezvous. It returns a
GUI-owned anonymous XPC endpoint to an independently launched, signed CLI. The
CLI then talks directly to the GUI. The broker owns no presentation state or
PDF authority; the renderer's existing XPC service remains a separate isolation
boundary.

The [App Group socket probe in #440](https://github.com/koichiro/quick-presenter/pull/440)
proved that signed sandboxed roles could communicate after TestFlight
installation. It also rejected the tested `LOCAL_PEERTOKEN` authentication
policy: a queued request from a wrong-identifier process was accepted after
that PID executed the approved Client. This fails the required binding between
a request and the code that sent it. It does not show that every Unix socket
scheme is impossible or affect the non-Store transport.

The [XPC diagnostic in #495](https://github.com/koichiro/quick-presenter/pull/495)
passed ten rendezvous, identity, and endpoint-lifecycle cases in an installed,
Apple-re-signed internal TestFlight build of version 2.5.0 (25001) on macOS 27
arm64. The result includes an independent CLI round-trip and rejection of wrong
signing identifiers. It proves the diagnostic transport in that environment.
The #495 diagnostic did not exercise the product GUI or Protocol v1 commands.
The optional product implementation now does so in a Developer ID-signed
local fixture; its commands, wrong-identifier rejection, and watch behavior
are recorded in [the #431 product validation](validation/ISSUE431_STORE_CONTROL_PRODUCT_20261020.md).
Apple-re-signed product validation remains open. See the
[exact build record](https://github.com/koichiro/quick-presenter/blob/cfb626a1e0a7672c5b395747bd77d66f7316c8c8/docs/validation/ISSUE431_STORE_XPC_25001.md)
for source and installed hashes, signatures, cases, and remaining limits.

## Product boundaries

```text
signed Store GUI --registers anonymous endpoint--> per-user XPC broker
signed Store qp  --looks up current endpoint------> per-user XPC broker
signed Store qp  --Protocol v1 requests-----------> Store GUI
Store GUI        --render requests----------------> existing PDF renderer
```

The GUI remains the only owner of presentation state and dispatches accepted
requests on its existing event loop. The broker must not relay commands, notes,
PDF paths, or document contents. GUI exit invalidates its registration; a new
GUI registers a new endpoint. A second GUI must not replace a live owner
silently. CLI lookup without a current GUI returns `NOT_RUNNING`.

The first product increment controls a running Store GUI and a PDF selected
through that GUI: status, notes, slide/context queries, elapsed time, navigation,
blackout, close, export, and watch. Timer access stays read-only. CLI-driven GUI startup
and arbitrary-path `presentation.open` require the file-authority design in
[#432](https://github.com/koichiro/quick-presenter/issues/432). Until then,
reject Store open/startup explicitly; CLI authentication alone gives no PDF
file authority. [#433](https://github.com/koichiro/quick-presenter/issues/433)
owns the final CLI package, invocation path, and distribution qualification.

## Identity and service lifecycle

Use a sandboxed per-user LaunchAgent registered through `SMAppService`, with a
separate broker executable and a `BundleProgram` launch-agent entry. Its
registration, approval, denial, upgrade, and removal states must be explicit.
App Group membership may permit the intended roles to reach the named service;
it is not itself a service registration or peer-identity check. Keep control
entitlements off the PDF renderer.

Every broker, GUI, and CLI XPC connection must install the native exact
same-team and peer signing-identifier requirement before activation. The
broker accepts endpoint registration only from the product GUI and lookup only
from the bundled CLI; the direct GUI endpoint accepts only that CLI. Reject a
wrong identifier even when its team and App Group match. Qualify user/session
routing separately from signing identity. A symlink may invoke the signed
bundled CLI, but an unsigned or rebuilt binary is not an authorized raw client.
Do not fall back to a same-user socket, PID-only check, shared secret, or broad
sandbox exception if XPC registration or identity verification fails.

The optional product package stages a dedicated App Group entitlement, nested
CLI and broker roles, and a per-user LaunchAgent. Its Developer ID-signed
development fixture passed local product integration. The available App Store
profile does not yet authorize that App Group, so Apple Distribution signing
and TestFlight installation of this product build remain open.

## Protocol and failure behavior

Translate Control Protocol v1 requests, replies, and events to bounded XPC
messages without changing public JSON/NDJSON output, error codes, or command
semantics. Preserve cancellation, watch ordering, full-text assembly, and
uncertain-mutation handling. An XPC send completion is not consumer
backpressure; explicit acknowledgement or bounded delivery is needed for
watch and large responses. Reject oversized or malformed messages before
allocating unbounded work.

Carry the existing ceilings into the XPC design: eight active clients, eight
pending requests, four watchers, 64 queued events per watcher, 1 MiB request
frames, and 32 MiB assembled responses. Measure deadlines, memory, descriptors,
CPU, and shutdown under slow or absent clients while PDF playback remains
responsive. Map a missing current endpoint to `NOT_RUNNING`, failed identity or
configuration to `IPC_FAILURE`, and elapsed deadlines to `TIMEOUT`; never
replay an uncertain mutation automatically. Logs should contain bounded,
redacted failure categories rather than notes, PDF paths, tokens, or profiles.

## Adoption gates

1. Integrate the broker, GUI endpoint, and signed CLI into the product package.
   Verify registration and approval on a clean user account, independent shell
   invocation, and update/removal behavior.
2. Verify exact peer identities on every connection and message, including
   wrong-identifier, foreign-team, unsigned, stale-endpoint, GUI-restart, broker-
   restart, and second-GUI cases. Prove the intended user/session boundary.
3. Exercise the supported Protocol v1 commands, responses, watch, full text,
   cancellation, error mapping, and output compatibility through XPC. Keep
   Store open/startup disabled until #432 supplies authorized file access.
4. Test concurrency and response limits, slow clients, shutdown, and
   presentation-length operation without disrupting playback.
5. Repeat signed-product checks on Apple-re-signed installed builds, including
   the minimum macOS 14.4 and Intel. Verify Developer ID macOS, Linux, and
   Windows transport regressions separately. #433 then validates the final
   customer-facing Store package and installation path.

Record source revision, OS/architecture, signing identities, package identity,
test cases, and limits in a redacted validation record. Keep private keys,
profiles, raw logs, audit tokens, personal paths, and account data out of Git.
Keep #431 and #422 open until the product route passes these gates.
