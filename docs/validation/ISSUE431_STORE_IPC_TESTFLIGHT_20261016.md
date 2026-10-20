# Store IPC TestFlight validation

Date: 2026-10-16 JST. [#440](https://github.com/koichiro/quick-presenter/pull/440)
now has successful IPC evidence from an Apple-re-signed TestFlight installation.
All eleven original diagnostic cases and four load/lifecycle cases passed on
macOS 27.0.1 / arm64. This qualifies the tested diagnostic layout's distribution,
container access, and selected mutual identity checks. It does not settle the
socket byte-origin limitation from the [previous experiment](ISSUE431_STORE_IPC_FOLLOWUP_20261015.md)
or qualify production GUI/Protocol v1 integration.

## Artifact and distribution identity

| Item | Value |
| --- | --- |
| Source published before upload | `f73794ac30aca6a336d42b6714d195af6db697c4` |
| Version / build | `1.9.0` / `19001` |
| OS / SDK / architecture | macOS 27.0.1 (26A434), macOS 27.0 SDK, arm64 |
| PDFium | Chromium 8076, same pinned version as the installed v1.4.0 baseline |
| Unsigned/ad-hoc GUI input SHA-256 | `32f404c5bce62db6b5eb900a1db7b2940a2675fca4eea2e122f0f6835d8459b6` |
| Unsigned/ad-hoc probe input SHA-256 | `e27b97c3ded5d6146aabf62b74c7e73a5349137b86775a4049b601898769fd8f` |
| Submitted signed installer SHA-256 | `81fdd735de3f1cae729497d5e87cb336c4870e923f3144b9bd15cf82587be3bd` |
| Delivery | Transporter, received 2026-10-16 09:10 JST |
| Apple processing | `COMPLETE`; no errors, warnings, or informational findings |
| Build status | `VALID`, internal `IN_BETA_TESTING` |
| Distribution audience | Existing Release Validation internal group, one existing tester |
| Installed signing authority | GUI and all four diagnostic helpers: `TestFlight Beta Distribution` |
| Installed root identity | `app.quickpresenter.QuickPresenter`, expected nonempty developer team |
| Group | `<same validated team>.app.quickpresenter.control-probe` |

The candidate uses the existing independently sandboxed GUI/renderer layout,
omits unsupported production `qp`, and adds `StoreIPCProbe.app` with nested
Client, WrongPeer, and MissingGroup apps. Only the diagnostic server/client/
wrong-peer receive the probe group; the GUI and renderer gain no control group.
The root embeds the existing product distribution profile. The four diagnostic
helpers had no individually embedded provisioning profiles. Apple accepted,
re-signed, and installed this exact arrangement; this is evidence for this
artifact and OS, not a general rule about every helper/profile configuration.

The installed GUI and helpers passed deep signature verification, individual
trusted Apple-anchor/exact-identifier requirements, matching nonempty team
checks, and Apple distribution-authority checks. A receipt was present. The
diagnostics ran directly from the installed signed bundles using only a
temporary symlink adapter. Installed bytes were not modified or re-signed.
The [machine-readable result](ISSUE431_STORE_IPC_TESTFLIGHT_20261016.json)
records each installed executable's distinct SHA-256 separately from the input
and installer hashes above.

The prior v1.4.0 build had `usesNonExemptEncryption=false`. The diagnostic
candidate retains PDFium 8076 and adds no non-exempt cryptography: the added
native checks use Apple's Security APIs and the IPC carries unencrypted JSON.
The same setting was applied after processing required export-compliance
metadata. This is a recorded configuration for this candidate, not a waiver
for future releases with different cryptographic dependencies or behavior.

The build was assigned only to the existing internal group. No external beta
review, customer review, or customer release was submitted. Transporter's
ordinary upload produced `APP_STORE_ELIGIBLE` audience metadata, not Xcode's
enforced Internal Only flag. Its test notes explicitly identify it as a
diagnostic that must not be selected for customer release. The local TestFlight
installation was updated to 1.9.0 (19001); public v1.4.0 release settings were
not changed.

## Runtime results

All original eleven cases passed against the Apple-produced installed helpers:

- Missing-group self rejection and raw ad-hoc input rejection.
- App Group lookup and socket bind.
- Mutual peer identity validation and a diagnostic roundtrip.
- Three ordered replies on one connection with private probe ACKs.
- Symlink invocation of the installed client.
- Wrong client identifier rejection and wrong server identifier rejection.
- Second-server exclusion.
- Eight requests with four concurrent callers.
- Verified stale-socket recovery after terminating the runner's server.

The added load/lifecycle runner also passed all four cases:

- Four subscribers reject a fifth while single-reply commands remain usable.
- Subscription slots recover after the clients exit.
- One hundred roundtrips with four concurrent callers return expected replies.
- A finite-lifetime server terminates with an active long subscription; the
  interrupted client reports failure rather than continuing with invalid data.

The JSON records the measured shutdown wait; sleeping between replies means
this is not a strict deadline or hung-Security-call guarantee. No PDF, notes,
GUI presentation state, or customer document was accessed by these tests.

## Checks and reproduction

`cargo fmt --check`, `cargo check --locked --offline`, the full
`cargo test --locked --offline`, and release GUI/CLI/example builds passed for
the candidate source. Full tests used the existing pinned PDFium dylib and a
local environment that permits sockets. The existing `fetch_update`
deprecation warning remains. Python compilation, `git diff --check`, nested
signature verification, installer signature verification, and the nine-Mach-O
private-import check passed.

The separately added installed-payload runner rejects a mismatched old build
before executing it. Run from this branch with the raw probe from this build:

```sh
python3 scripts/check_macos_store_ipc_testflight.py \
  '/Applications/Quick Presenter.app' target/release/examples/store_ipc_probe 19001
```

Use the actual TestFlight installation path. The runner checks the requested
build, receipt, signatures, identities, distribution authorities, and team,
then runs both existing suites. It normalizes their older fixed scope labels
for this verified Apple-produced payload; their diagnostic payloads and ACKs
are unchanged. Packaging and installation instructions are in the
[probe guide](../STORE_CLI_IPC_PROBE.md#diagnostic-testflight-candidate).

## Decision and remaining gates

This closes the absence of any Apple-re-signed IPC evidence for the exact
diagnostic layout on the tested Mac. It supports the feasibility of App Group
sockets and the selected signature checks under TestFlight re-signing.

Socket adoption remains conditional. Equal `LOCAL_PEERTOKEN` snapshots are not
a general proof of queued-byte origin across shared endpoint holders. Review
the real descriptor threat model and the per-message XPC alternative before
production integration. TestFlight success does not change that kernel API
property or add the private probe ACK to Protocol v1.

macOS 14.4, Intel, a separately installed customer Store artifact, foreign-team
peers, exec/PID/SCM_RIGHTS adversarial cases, long container paths, eight-worker
saturation, prolonged resource/slow-reader checks, Security-call latency and
shutdown, real GUI/Protocol v1 behavior, and #432 startup/PDF authority remain
open. The original no-ACK peer-lifetime issue also needs a production response
contract. Keep #431, #422, and the applicable #432/#433 gates open.
