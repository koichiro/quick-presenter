# Issue 431: 2.5.0 internal distribution-build check

Date: 2026-10-20. Source commit:
`94833eefdcd63026b3d23b23497fcb922604630d` on PR #495, rebased onto
`main`. This is an internal TestFlight diagnostic candidate, not a customer
release or an implemented production Store CLI.

## Candidate assembled

The release-profile GUI, CLI, and socket diagnostic were built from this
checkout with `cargo build --release --locked --offline --bins --example
store_ipc_probe`. The Objective-C XPC diagnostic was compiled from this
checkout with `-fblocks -Wall -Wextra -Werror`. All inputs are arm64 Mach-O
executables. The release CLI reported `application_version: 2.5.0` and
`protocol_version: 1`.

| Input | SHA-256 |
| --- | --- |
| Release GUI | `70b78f8c0870964efaa15b09a6cb3cc430bee6931379ac0af0a2d786de610274` |
| Release CLI | `706f0dd5dea828f3c6e5ad443c934d04f0fe8e44b58a6eaf075d209b047e1c39` |
| Socket diagnostic | `5008204caccafa16d0da8cfb693f7ad4e1f70bbb048750432c738491e23364f8` |
| XPC diagnostic | `1c98da7f10736d02a6b45f92bc8d88109ae637f7b1083e5ba4ec34846fcde746` |

The internal package was created with
`scripts/package_macos_store_ipc_testflight.py` for version 2.5.0, build
25001. Its root product app retains the existing renderer boundary and has no
production `qp` executable or control group. The separately signed diagnostic
roles are under `Contents/Helpers`. The package script verified 13 Mach-O
files had no private CGS imports, performed a deep strict code-signature check,
and checked the signed installer. The installer SHA-256 is
`4f0878f291d9b977605cd0c9a18eaeddf71b71e1049c47bbfbc4959f8fedf097`.
The build manifest at the staging destination records the exact inputs.

## Apple processing and installed gate

Transporter delivered build 25001 on 2026-10-20. App Store Connect processed
it to `VALID`. The existing single-tester internal TestFlight group received
this build; it was not assigned to an external group or submitted for customer
release. TestFlight installed `/Applications/Quick Presenter.app` from this
build. The installed verifier was run as:

```sh
python3 scripts/check_macos_store_xpc_testflight.py \
  '/Applications/Quick Presenter.app' 25001 --version 2.5.0
```

It passed on macOS 27 arm64. The installed payload has a TestFlight receipt,
valid deep strict code signatures, the expected Apple distribution authority,
exact product GUI and four diagnostic helper identifiers, one team
(`NP5Q6U6NT5`), and the expected sandbox and App Group entitlements. The
product GUI has no App Group entitlement. The Apple-re-signed product GUI
SHA-256 is
`db55a7150f9e27cddb907140acbfbd62fe5aa10f4d1a579c0e8aa86e3167bc78`.
The four installed diagnostic executable SHA-256 hashes are:

| Role | SHA-256 |
| --- | --- |
| GUI | `bad38e3088444130a7978b58acd6902271c1da3712c38cdaa6fc9fad0cb764ee` |
| Broker | `1f1457735915604a98a0346fd052bc2ff805d43ad09e545489b0ca47e92c125e` |
| Client | `84b8f01f40eb8e80837f4303eba80fd6b48a6906ad8730f4933101dc32c4cedd` |
| WrongPeer | `e847fd428bf692a08ef04b8d7d22b6d245aa9c38a16de34bce12ae6fd8fa5f30` |

The installed diagnostic passed all ten cases: unregistered-service
rejection, ServiceManagement registration, anonymous GUI endpoint
registration, independent CLI direct round-trip, wrong client identifier
rejection, wrong GUI identifier rejection, second GUI registration rejection,
stale endpoint rejection after GUI exit, new endpoint after GUI restart, and
agent removal after testing. The agent was unregistered at the end.

The installed product GUI opened `docs/samples/quick-presenter-demo.pdf`,
rendered page 1 of 12 with speaker notes and next-slide preview, navigated to
page 2 and back to page 1, and reopened the same PDF after an app restart. On
the first launch after replacing the older TestFlight build, the app displayed
an error about reopening a previous PDF. Selecting the sample PDF recovered;
the subsequent restart restored it successfully. This does not establish a
general migration failure, but the stale-document path warrants a separate
reproduction if it recurs.

## Remaining validation

The diagnostic establishes that this XPC rendezvous works in one
Apple-re-signed internal distribution build. Product GUI integration and
Protocol v1 command handling are not implemented by the probe. macOS 14.4 and
Intel compatibility, resource limits, repeated presentation-length runs, and
reviewer-facing App Store distribution remain unverified.
