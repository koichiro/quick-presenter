# Issue 431: product Store CLI control integration

Date: 2026-10-20. This implementation branch starts from `main` at
`fa002df5d8a59607ebb8f606b8e859fd41f46f95`, after #495 was merged.
The optional `macos-store-control` build replaces the macOS Store GUI/CLI
Unix socket with an authenticated XPC connection through a per-user broker.
The normal Developer ID/DMG and other platform builds retain their existing
transport. The product retains Control Protocol v1 and its state dispatcher.

The signed development fixture used the v2.5.0 release binaries built on
macOS 27.0.1 arm64 with SDK 27.0. Each role had its exact signing identifier,
App Sandbox, and the same control App Group. The GUI registered its endpoint
with `SMAppService`. The binary input SHA-256 values were:

| Role | SHA-256 |
| --- | --- |
| GUI | `b9a2058873b0c70033583699fcefe02ff3dd249c8b4c29da35437b4620010ba0` |
| CLI | `68162113d8b67f4a59f88cd411c60910f85873f96d2b089513be05b13378e8da` |
| Broker | `0988eaa0b5f1f17429c478880d913268867b005cab67116c1b6cdf141e4b645e` |

`codesign --verify --deep --strict` passed for the Developer ID-signed app.
The product integration runner passed status, timer/notes/slide/context/full
queries, all-page export, navigation, blackout, watch notification, close,
and 32 repeated CLI connections. A same-team and same-group binary with the
wrong signing identifier returned `IPC_FAILURE`. Raw-path `qp open` returned
`UNSUPPORTED_PLATFORM`. After GUI exit, `qp status` returned `NOT_RUNNING`
with exit code 3. Restarting the GUI registered a new endpoint and `qp status`
again succeeded. The watch check observed the initial snapshot and a page
change from a separate CLI process. These tests used the actual Slint GUI
with a 12-page PDF and the product broker, not the #495 diagnostic helper.

`cargo fmt --check`, `cargo check --locked --offline`, and
`cargo test --locked --offline` passed for the normal build. The feature
build passed `cargo check --locked --offline --features macos-store-control
--bins`, `cargo test --locked --offline --features macos-store-control`, and
`cargo build --release --locked --offline --bins --features
macos-store-control`. The native XPC source compiled with warnings treated
as errors for arm64 and x86_64. Socket-only integration tests run only for
the normal build; Store transport integration is covered by the signed
fixture above.

The initial package script staged the feature binaries and found no private
CGS imports, but stopped before signing because it required the existing App
Store profile to list `TEAMID.app.quickpresenter.control`. Apple documents
that this macOS Team ID-prefixed group needs no separate registration. The
package gate was removed, and the product TestFlight build 25002 passed Apple
processing and installed runtime validation. See
`docs/validation/ISSUE431_STORE_CONTROL_TESTFLIGHT_25002.md` for the exact
distribution result. The earlier #495 TestFlight build 25001 established
only the diagnostic XPC route.

Remaining gates are clean-account registration, approval/denial and upgrade
behavior; foreign-team and unsigned peers against the Apple-distributed
broker; slow-client and presentation-length load; macOS 14.4 and Intel runtime
checks; final Store package inventory and signing; App Store review; and the
authorized `qp open` file handoff tracked in #432. Do not claim customer Store
CLI support from this internal TestFlight build alone.
