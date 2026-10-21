# Store IPC follow-up validation

The subsequent [2026-10-16 TestFlight record](ISSUE431_STORE_IPC_TESTFLIGHT_20261016.md)
qualifies the installed diagnostic IPC layout. The sign-in/distribution blocker
recorded below is historical; the descriptor-origin limitation remains open.

Date: 2026-10-15 JST. The maintainer reported approval and availability of Store
v1.4.0 and requested resuming #440 validation. The previous hold is lifted.
The socket adoption gate remains open: the new descriptor experiment shows
that equal pre/post `LOCAL_PEERTOKEN` values do not establish queued-byte origin
when another process shares the endpoint. No normal `qp` exploitation path is
demonstrated. TestFlight IPC qualification still requires a diagnostic build.

## Tested artifacts

| Item | Identity |
| --- | --- |
| Existing Rust probe source | `b4509508a58b502d5be3cccbd1351035112a0459` from #440 |
| OS / SDK / architecture | macOS 27.0.1 (26A434), macOS 27.0 SDK, arm64 |
| Native compiler | Apple clang 21.0.0 (`clang-2100.3.34.2`) |
| Rebuilt Rust input SHA-256 | `cb460dd33ffac5293de70c1e03259b9ccd6e37df49d145d0cbc55658984c2ba5` |
| Descriptor fixture input SHA-256 | `36e2bd1db38d98ca772f800428b6540b9696a67a154cf54e63e8d8f0e4aa4303` |
| Descriptor fixture source SHA-256 | `53b4e0f20ac9f3572a6ef63b4d6006b1d582d8e9409d044c5dd46a356f9d0221` |
| Load runner source SHA-256 | `b6adf47eb435f2545fa6bbf26c0cd4663da7575ca152bd62687631eb4c4f687f` |
| Identity inspector source SHA-256 | `4853df8167e29f25d0e1372d625675591094ec49a60bc86ba4539b083b5e42f6` |
| Local sandbox packages | Separate fresh diagnostic directories under `/Applications`; existing product apps unchanged |
| Local signing | Existing Developer ID identity, runtime flag, independent sandbox and dedicated probe group; no notarization or Store upload |
| Installed Apple-signed app | v1.4.0 (14005), `app.quickpresenter.QuickPresenter`, `TestFlight Beta Distribution` authority, receipt present |

Input hashes identify binaries before the package script re-signs them. Fixture
source hashes identify the added tests without treating their later commit as
the original Rust probe revision. TestFlight displayed v1.4.0 (14005) as an
available update, expiring 2027-01-07. No update was installed during these tests.
The installed payload's signature establishes TestFlight provenance; it is not
evidence from a separately installed customer Store payload.

## Signed IPC and lifecycle results

The Rust example build and its seven unit tests passed with `--locked --offline`.
Both added native utilities compiled with `-Wall -Wextra -Werror`; the Python
runner passed compilation and the diff passed whitespace checks. Production
Rust/Slint/build configuration is unchanged, so the full Cargo suite was not
rerun for this follow-up.

The rebuilt and freshly signed Rust probe passed all eleven original runner
cases again. The added `check_macos_store_ipc_load.py` passed:

- Four active 30-reply subscriptions rejected a fifth subscription while a
  single-response request remained usable. The server log identified the
  watcher-limit gate.
- After those clients exited, a new two-reply subscription succeeded within the
  bounded retry window, demonstrating watcher-slot recovery.
- One hundred single-response requests with four concurrent callers returned
  the expected diagnostic reply.
- A server with a three-second lifetime exited with code 0 while a 30-reply
  subscriber remained connected. The wait after the first response was 4.026
  seconds; the client exited 7 on interruption. This is observed termination,
  not a strict three-second wall-clock shutdown guarantee.

This small diagnostic run does not qualify eight-worker saturation, prolonged
resource growth, slow readers, native Security-call hangs, GUI responsiveness,
or real Protocol v1 response/watch behavior. No resource-soak claim is made.

## Socket token semantics

`peer_token_semantics.c` uses only a local Unix stream socket pair and pipes:

1. Record the parent endpoint owner's audit token.
2. Fork a child sharing the sender endpoint. The child writes one diagnostic
   byte, then stays alive while pipes coordinate the observation.
3. The receiving endpoint initially reports the child's token.
4. The parent calls nonblocking `recv(MSG_PEEK)` on the shared sender endpoint.
   It gets `EAGAIN` and writes no bytes.
5. The receiving endpoint now reports the parent's original token, even after
   reading the byte that the child queued.

All four boolean checks were true in 30 raw runs and 30 Developer ID-signed
App Sandbox runs with the dedicated probe-group entitlement. Exit 0 means
**the attribution limitation was reproduced**, not that authentication passed.
No raw audit tokens or process IDs are printed. Each run has a five-second alarm.

This agrees with Apple's published
[XNU implementation](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/uipc_usrreq.c):
`LOCAL_PEERTOKEN` looks up the peer socket's current `last_pid` and task. It is
not metadata attached to each received byte. The experiment therefore refutes
using a matching pair of token snapshots as a general proof of byte origin
across shared endpoint holders. A successful ordinary signed roundtrip or
Apple re-signing cannot establish that missing binding.

The fixture intentionally allows descriptor sharing and cooperation by the
original holder. It does not demonstrate that an unrelated process can obtain
an actual product descriptor, bypass signing checks, or exploit unmodified
`qp`. Both processes also have the same code identity. Foreign identifiers,
foreign teams, exec, SCM_RIGHTS transfer, and PID reuse are not covered by this
fixture. Production uses close-on-exec descriptors and does not intentionally
fork or transfer them; whether those restrictions suffice for the stated threat
model still needs explicit review. Keep the socket gate unpassed and evaluate
the per-message XPC identity alternative before production integration.

## Apple-re-signed identity inspection

The small `inspect_store_identity.m` utility checked the installed TestFlight
payload with Security APIs. The trusted Apple anchor plus expected bundle
identifier requirement passed (`OSStatus == 0`); signing metadata retained the
expected developer Team ID and identifier. The entitlement dictionary was
available and reported App Sandbox enabled with no App Groups.

Running inside the agent's own restricted environment initially prevented
trust evaluation and entitlement decoding. Repeating outside that restriction
returned success. A `codesign` legacy entitlement-dump warning alone is not
evidence that the installed app's runtime entitlements are invalid.

This validates only the static identity-policy subset on one existing
Apple-re-signed artifact. The app contains neither #440 nor its probe group, so
it cannot qualify container access, mutual dynamic peer checks, or IPC. Do not
modify/re-sign the installed app to manufacture such evidence.

## Reproduction

Build/sign the Rust probe and run the original runner as documented in
[the probe guide](../STORE_CLI_IPC_PROBE.md), then run:

```sh
python3 scripts/check_macos_store_ipc_load.py /Applications/QuickPresenterIPCProbe
xcrun clang -Wall -Wextra -Werror \
  examples/store_ipc_probe/peer_token_semantics.c -lbsm -o /tmp/qp-peer-token
/tmp/qp-peer-token
scripts/package_macos_store_ipc_probe.sh \
  /Applications/QuickPresenterIPCTokenProbe /tmp/qp-peer-token \
  'Developer ID Application: Account Name (TEAMID)' TEAMID
/Applications/QuickPresenterIPCTokenProbe/Probe.app/Contents/MacOS/probe
xcrun clang -Wall -Wextra -Werror \
  examples/store_ipc_probe/inspect_store_identity.m \
  -framework Foundation -framework Security -o /tmp/qp-store-identity
/tmp/qp-store-identity '/Applications/Quick Presenter.app'
```

Use fresh destinations, actual identity/team, and the actual installed app path.
The token fixture needs no command arguments; the packaging script's helper
copies are unused. These are local diagnostics, not submission packages.

## TestFlight continuation and remaining gates

App Store Connect opened at its sign-in page; the authenticated session was not
available to this run, and the maintainer was asked to sign in. An existing
unexpired distribution profile for the product Bundle ID was found, but it
does not establish authorization/processing for the new diagnostic helper
identifiers. No new identifiers, permissions, profiles, tester access, uploads,
review submissions, or release settings were changed.

For an actual TestFlight IPC run, prepare an explicitly diagnostic candidate
that includes independently sandboxed server/client helpers and the dedicated
group, preserves the GUI/renderer signing boundary, and records the exact
source/build/package identity. Verify every helper's provisioning context and
Apple's processed layout. Use internal testing; do not release the diagnostic
build to customers. Run the mutual identity, negative-peer, group, lifecycle,
and token tests against the installed Apple-produced payload and retain its
identity separately from the upload input.

macOS 14.4, Intel, foreign-team/exec/PID cases, eight-worker saturation, long
paths, resource/native-call measurements, GUI/protocol integration, and #432
PDF/startup authority remain open. Approval of v1.4.0 lifts the hold; it does
not approve the future IPC entitlements, layout, or transport. Keep #431/#422
open. No socket or XPC production implementation is selected by this run.
