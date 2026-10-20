# Mac App Store CLI XPC diagnostic

This is a development-only proof for the XPC route selected by
[the #431 design](https://github.com/koichiro/quick-presenter/pull/434).
It does not control a presentation or open a PDF. It keeps the product GUI,
CLI, and renderer boundaries unchanged. The diagnostic roles use distinct
signed bundles:

| Role | Signing identifier | Responsibility |
| --- | --- | --- |
| GUI | `app.quickpresenter.xpc-probe.gui` | Register a GUI-owned anonymous XPC endpoint. |
| Broker | `app.quickpresenter.xpc-probe.broker` | Publish two per-user LaunchAgent Mach services and return the current endpoint. |
| Client | `app.quickpresenter.xpc-probe.client` | Discover the endpoint and send a diagnostic ping directly to the GUI. |
| WrongPeer | `app.quickpresenter.xpc-probe.wrong` | Negative tests for both registration and lookup. |

The broker advertises separate group-prefixed `.register` and `.lookup`
services. This lets each listener install an exact peer signing-identifier
requirement before activating accepted connections. All three communication
legs also require the same signing team. The GUI accepts only the Client on
its anonymous endpoint; the Client accepts only the GUI on the direct
connection. `xpc_connection_set_peer_team_identity_requirement` is available
on the Store minimum macOS 14.4 and checks every received message. The broker
holds no presentation or PDF state. It clears its stored endpoint when the
owning GUI connection closes and rejects a second GUI registration.

The diagnostic uses a single `ping`/`accepted` exchange. It does not implement
Presentation Control Protocol v1 framing, full-text assembly, watch, bounded
backpressure, or production startup/file authority. Its synchronous diagnostic
calls are bounded by the external runner; a production wrapper needs native
timeouts and cancellation behavior. Do not reuse the renderer XPC service.

## Comparison with the v2.0.0 control transports

The public `qp` commands, JSON and NDJSON output, error codes, and Control
Protocol v1 are independent of the transport. The XPC proposal changes how
the Mac App Store CLI finds and authenticates the GUI; it does not propose a
new user-facing protocol. The v2.0.0 implementation and its distribution
limits are described in [the control contract](CONTROL_PROTOCOL.md) and
[CLI installation guide](CLI.md#installation).

| Route | Connection and trust boundary | Benefit | Cost or limitation |
| --- | --- | --- | --- |
| v2.0.0 macOS Developer ID and Linux | Per-user Unix-domain `control.sock`, a 0700 directory, 0600 socket, and a single-owner lock | Simple Rust byte stream; the existing 1 MiB framed JSON, deadlines, watch, and bounded queues work directly | Endpoint discovery depends on a shared runtime path. Owner and mode checks authenticate a user boundary, not the signing identity of each request. It is not a supported Mac App Store endpoint. |
| v2.0.0 Windows | Local named pipe with the creator-account DACL, remote-client rejection, server-user verification, and identification-level impersonation | No socket path or stale filesystem inode; preserves the same CLI contract | Windows-specific implementation and security policy; XPC does not replace it. |
| Rejected Store App Group socket probe (#440) | Group container socket with signed, sandboxed roles and a `LOCAL_PEERTOKEN`-based identity check | Both signed roles can reach a shared endpoint after Apple re-signing; it could reuse the Unix byte-stream server | The tested check accepted an already-queued request from a wrong-identifier process after that PID executed the approved Client. Container access and a current-PID check do not establish the sender identity of that request. |
| Proposed Store XPC route (#495) | A per-user `SMAppService` broker returns a GUI-owned anonymous endpoint. Each XPC leg applies an exact same-team/signing-identifier requirement before activation | Installed TestFlight build 19003 passed independent CLI roundtrips, wrong-identifier rejection, and endpoint-lifecycle checks without a filesystem control socket | Adds signed nested roles, an agent registration/approval and update lifecycle, and macOS-specific XPC bindings. The diagnostic exchanges only `ping`; Protocol v1, cancellation, watch/full-text streaming, startup, and load limits still need product integration. |

The broker holds only the current endpoint; the GUI remains the sole owner of
presentation state and PDF file authority. The XPC diagnostic establishes a
viable transport and identity boundary on one macOS 27 arm64 TestFlight
installation, not a finished Store CLI. The product implementation must keep
the v2.0.0 CLI contract and its bounds while translating requests and
responses to XPC messages. The existing Unix socket and Windows named pipe
remain appropriate for their supported distributions; this proposal changes
only the Mac App Store route. The Store socket failure and XPC pass are
recorded in [the exec-transition test](validation/ISSUE431_STORE_IPC_EXEC_20261020.md)
and [the XPC TestFlight test](validation/ISSUE431_STORE_XPC_20261020.md).

## Local signed run

Use a fresh absolute destination in `/Applications` and an existing Developer
ID Application identity. No certificate or profile is created:

```sh
python3 scripts/package_macos_store_xpc_probe.py \
  /Applications/QuickPresenterXPCProbe-NEW \
  examples/store_xpc_probe.m \
  'Developer ID Application: Account Name (TEAMID)' TEAMID
python3 scripts/check_macos_store_xpc_probe.py \
  /Applications/QuickPresenterXPCProbe-NEW
```

The runner requires the diagnostic LaunchAgent to be initially unregistered.
It registers through `SMAppService`, starts only its own GUI role, checks
positive and negative paths, and unregisters the agent in `finally`. The
registration may need approval on another machine; a denied or
approval-required state is an explicit failed gate, not a reason to install a
private launchd job manually. See the
[2026-10-20 local record](validation/ISSUE431_STORE_XPC_20261020.md).

## Internal TestFlight candidate

`package_macos_store_ipc_testflight.py` accepts optional `--xpc-probe RAW`.
This stages an independent `StoreXPCProbe.app` under the existing internal
diagnostic product, plus its three nested roles and LaunchAgent plist. Only
the diagnostic XPC roles receive the probe group. The product GUI and renderer
receive no control group. The package is for internal validation only, never
customer release. After Apple processing and installation, run:

```sh
python3 scripts/check_macos_store_xpc_testflight.py \
  '/Applications/Quick Presenter.app' BUILD_NUMBER
```

The installed verifier requires the exact build, receipt, deep signature,
Apple distribution authorities, four exact identifiers, one consistent team,
and the expected sandbox/group entitlements before running the local suite
through a temporary symlink adapter. It does not alter installed bytes. Do
not claim Store feasibility from a locally Apple Distribution-signed candidate
without this Apple-re-signed installed check.

Internal build 19003 passed all ten cases after TestFlight installation on
macOS 27 arm64; the exact evidence and remaining gates are in the validation
record. The TestFlight LaunchAgent uses a distinct label from the Developer
ID fixture, avoiding the cached launch constraint mismatch seen in build
19002.
