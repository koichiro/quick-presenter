# Issue 431: merged v2.5.0 Store CLI TestFlight build 25005

Date: 2026-10-21. The signed candidate was built from the `v2.5.0` merge
commit `e86ea232f84ea974c2993b2e89ccefdc88329033` (PR #496), version 2.5.0,
build 25005, on macOS 27.0.1 arm64 with Xcode 27.1 and pinned PDFium
`chromium/8076`. This is the first TestFlight candidate built from the merged
release branch rather than PR #496's original main-based source.

Local validation passed `cargo fmt --check`, `cargo check --locked`,
`cargo test --locked`, Store-control feature checks and tests, and
`cargo build --release --locked --bins --features macos-store-control`.
The three release binaries passed the private CGS import scan. The signed app
passed deep strict signature verification, exact GUI/CLI/broker identity,
same-team App Group entitlement, control-symbol, and LaunchAgent layout checks.
All seven bundled Mach-O files passed the private CGS import scan. The installer
signature passed `pkgutil --check-signature`.

The signed installer SHA-256 is
`3c37ffb898ab6414f283b6342729ecdb52ef608dfb7c50c1513d4c08f06eb782`.
Transporter delivered build 25005 to App Store Connect at 2026-10-21 00:51 JST
and subsequently reported processing complete. In App Store Connect, the same export compliance
classification used for prior candidates (no non-exempt encryption) was saved,
the build reached Ready to Submit, and it was assigned only to the existing
internal `Release Validation` group with one tester. The test notes identify
this as an XPC broker validation build and exclude it from customer release.
App Store Connect then showed build 25005 as Testing in that group with one
invitation. After propagation, TestFlight offered 25005 as an update to 25004
and installed it at `/Applications/Quick Presenter.app`. The installed app has
`CFBundleVersion=25005` and an App Store receipt. Deep strict signature
verification passed. The GUI, CLI, and broker each have the expected exact
identifier, Team ID `NP5Q6U6NT5`, sandbox and App Group entitlements, and a
TestFlight Beta Distribution → Apple Worldwide Developer Relations → Apple
Root CA certificate chain. The installed control symbols and bundled
LaunchAgent layout passed `validate_product(app, True)`.

The manually bootstrapped 25004 broker job had been removed before installing
25005 to prevent a stale process from masking the test. On first launch,
`launchctl` had no job for its label and the installed CLI returned
`NOT_RUNNING`; the GUI attempted to connect to the register Mach service but
launchd reported that service absent. Background Task Management still showed
the item enabled. This is consistent with the out-of-band `launchctl bootout`
leaving an enabled registration without a loaded job; the evidence does not
establish how a clean installation behaves. Direct `launchctl bootstrap` of
the bundled plist failed with I/O error. A local test plist with the same label
and Mach services and an absolute `Program` pointing to the **installed 25005
broker binary** bootstrapped successfully. The GUI was then restarted.

With the 12-page demo PDF opened through the installed GUI's file picker,
`python3 scripts/check_macos_store_control.py '/Applications/Quick Presenter.app' --testflight`
passed all seven reported groups: `status_query`, `read_only_queries`,
`raw_open_rejected`, `watch_and_navigation`, `goto_and_blackout`, `close`, and
`repeated_connections`. This exercised the Apple-re-signed GUI, CLI, and
broker, including the actual status, timer, notes, slide, context, export,
watch, navigation, blackout, and close commands. After quitting the GUI, the
CLI returned `NOT_RUNNING` with exit code 3. Reopening from TestFlight produced
a new session ID and a successful status query.

The result validates product XPC communication and CLI behavior on this
Apple-distributed arm64 build **after manual broker registration**. It does
not validate automatic `SMAppService` registration from a clean user session.
That setup path should be checked without a prior manual `launchctl bootout`
before claiming unattended first-launch behavior or customer readiness.
