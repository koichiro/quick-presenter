# Mac App Store IPC probe

This development-only experiment makes the initial socket proposal in
[design PR #434](https://github.com/koichiro/quick-presenter/pull/434) executable
before that PR is merged. It targets [#431](https://github.com/koichiro/quick-presenter/issues/431)
on `v2.5.0`. It does not change production GUI/`qp` routing, enable Store
presentation control, open PDFs, or claim the socket identity adoption gate has
passed.

## Implemented experiment

The `store_ipc_probe` Cargo example runs as a server or client. Its small native
bridge resolves a team-prefixed App Group container through Foundation and
checks self/peer code through Security. The probe requires macOS 14.4 or later,
Apple-trusted signatures, independent App Sandbox, and exactly one probe App
Group. Peer checks pin the opposite signing identifier and require the same
team and effective user. No user-selected file, bookmark, network, or sandbox
inheritance entitlements are permitted in these probe packages.

The group is `<TEAMID>.app.quickpresenter.control-probe`, separate from any
production group. The endpoint is `c/s` inside the container returned by
`NSFileManager`. A private lock prevents a second server from taking over;
verified stale sockets can be recovered. Unsafe permissions, wrong file types,
and socket paths longer than Darwin's address capacity fail explicitly. The
group root's permissions are not changed. No environment endpoint override,
unsigned bypass, PID-only fallback, `/tmp` fallback, or broad sandbox exception
is available.

Clients authenticate before sending. Servers authenticate before reading and
again after reading, then before every response. Clients check again before
printing each response. Kernel-reported `LOCAL_PEERTOKEN` audit tokens identify
the dynamic code for `SecCodeCopyGuestWithAttributes`; `SecCodeCheckValidity`
checks an Apple anchor and exact peer identifier. Validated signing metadata
must match the team and probe entitlements. A changed token ends the connection. Revalidation cannot identify a peer after
it has exited; the probe therefore keeps both peers alive through the reply ACK.
No raw tokens, personal paths, or PDF data appear in probe output.

The private payload is `{ "probe": 1, "sequence": 0, "count": N }`. This is
**not** a Presentation Control Protocol method or presentation state. It uses
the same four-byte big-endian framing writer, with a smaller 4 KiB diagnostic
read limit. A client requests 1–30 replies; subsequent replies are two seconds
apart and stdout is clean NDJSON. The server runs for 1–300 seconds, limits
workers to eight and multi-reply connections to four, and performs synchronous
writes with a one-second total frame budget using nonblocking IO and `poll`. There is
no unbounded event queue. Each reply is acknowledged with the same diagnostic
message, keeping the server alive while the client revalidates it. This ACK is
private to the probe and is not added to Protocol v1.
Client heartbeat arrival waits at most three seconds; complete-frame IO still
has its own one-second total budget. Native Security calls are synchronous and
not claimed cancellable; fixed worker counts bound allocations, but a hung
native call can delay shutdown. Resource/latency qualification remains a gate.

## Build and package

Use a development Mac with a trusted signing identity already installed in
Keychain. No new certificate or profile is created by this script.

```sh
cargo fmt --check
cargo check --locked
cargo test --locked
cargo build --locked --example store_ipc_probe
scripts/package_macos_store_ipc_probe.sh \
  /Applications/QuickPresenterIPCProbe \
  target/debug/examples/store_ipc_probe \
  'Developer ID Application: Account Name (TEAMID)' TEAMID
```

Pass the actual ten-character Team ID and an absolute destination that does not
exist. Creating the dedicated `/Applications` directory may require local
permission. The runner can also be used against other package locations, but
location is part of the experiment: the tested `/private/tmp` nested layout
failed peer code lookup while `/Applications` passed. Do not assume these
layouts have equivalent sandbox file authority. The packaging script refuses an existing destination, copies the input
binary without modifying it, creates one `Probe.app` with three nested helper apps, then
signs the nested apps before signing and verifying the outer bundle. It records the input binary SHA-256. The native archive is linked
only by this example; production executables do not reference the probe bridge.
The packaging script is for local Developer ID-signed sandbox tests, not Store
upload, notarization, release builds, or an Apple-re-signed qualification result.

| Executable inside `Probe.app` | Signing identifier | Purpose |
| --- | --- | --- |
| `Contents/MacOS/probe` | `app.quickpresenter.ipc-probe.server` | Accepted server identity. |
| `Contents/Helpers/Client.app/Contents/MacOS/probe` | `app.quickpresenter.ipc-probe.client` | Accepted client identity. |
| `Contents/Helpers/WrongPeer.app/Contents/MacOS/probe` | `app.quickpresenter.ipc-probe.wrong` | Trusted same-team/group peer with a rejected identifier, in either direction. |
| `Contents/Helpers/MissingGroup.app/Contents/MacOS/probe` | `app.quickpresenter.ipc-probe.missing` | Trusted sandboxed code without probe-group authority; rejected during self checks. |

For negative tests, self checks deliberately permit a trusted signature with a
different identifier. Remote identity checks always pin the expected opposite
identifier. This test fixture is not the production self-identity policy.

## Run and reproduce

Run the binaries directly from their signed bundles in Terminal; `open` is not
used for these command-line fixtures. In one terminal:

```sh
/Applications/QuickPresenterIPCProbe/Probe.app/Contents/MacOS/probe server 60
```

In another:

```sh
/Applications/QuickPresenterIPCProbe/Probe.app/Contents/Helpers/Client.app/Contents/MacOS/probe client 1
/Applications/QuickPresenterIPCProbe/Probe.app/Contents/Helpers/Client.app/Contents/MacOS/probe client 5
/Applications/QuickPresenterIPCProbe/Probe.app/Contents/Helpers/WrongPeer.app/Contents/MacOS/probe client 1
```

The server prints a `listening` record and writes bounded diagnostics to stderr.
When authentication succeeds, clients print exactly the requested ordered replies. Failure exits
7 with a JSON error on stderr; authentication failure prints no payload.
Native `stage` values distinguish unsupported OS (1), code/requirement creation
(2), signature validation (3), signing metadata (4), identity/entitlement policy
(5), container lookup (6), user credentials (7), audit-token lookup (8), and
dynamic guest lookup (9). `status` records an OSStatus when one applies.

Run the automated signed-package checks with no server already running:

```sh
python3 scripts/check_macos_store_ipc_probe.py \
  /Applications/QuickPresenterIPCProbe target/debug/examples/store_ipc_probe
```

The runner first verifies missing-group and raw-code rejection and signed server
startup, then attempts mutual authentication/roundtrip, a three-reply stream, symlink
invocation, wrong client/server identifiers, missing group, raw ad-hoc code,
second-server exclusion, eight commands with four concurrent callers, and recovery after terminating
its own server. It starts finite-lifetime servers, stops only processes it owns,
and outputs the passed cases plus remaining gates. On a failed prerequisite it
exits 1 with a JSON summary, leaves later cases unqualified, and never relaxes
the identity checks. See [local runtime results](validation/ISSUE431_STORE_IPC_PROBE.md)
for the qualified installed layout and failed staging/loose-binary layouts. It does not modify the signed
payloads. The probe directory/lock may remain in the dedicated group container
afterward; no production data or endpoint is used.

Rust tests cover byte-length limits, invalid/unbounded options, frame-length
rejection, partial-frame deadlines, watcher-slot cleanup, stale recovery, and
preservation of unexpected endpoint files. These tests are not native identity
proofs.

## Interpretation and remaining gates

A successful roundtrip shows that this signed configuration can resolve the
container, establish a socket, and pass the selected dynamic signature checks.
It does not prove that a socket token is tied to each received byte or that
another process cannot substitute itself. Apple's
[XNU implementation](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/uipc_usrreq.c)
derives `LOCAL_PEERTOKEN` from a socket PID/task lookup. Rechecking token values
does not eliminate all PID reuse, exec, FD transfer, or check/use races. Keep
the socket-versus-XPC decision conditional as specified in #434.

Before adopting this route, validate the supported OS range, foreign-team peers,
PID/exec/FD adversarial cases, long container paths, worker/watcher saturation,
slow readers, resource growth, native-check latency/shutdown, Apple-re-signed
TestFlight/Store identity, and real GUI/Protocol v1 integration. This experiment
has no GUI event loop, full-text response assembly, presentation event queue,
or PDF access handoff; it cannot qualify those behaviors. #432 and #433 retain
their respective startup/file-authority and distribution gates.

Record exact source revision, binary hash, OS/SDK, signed identifiers and group,
successful cases, and failures under `docs/validation/`. Keep private signing
files, raw tokens, local log paths, and certificate material outside Git.
