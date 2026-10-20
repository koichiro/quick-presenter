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
and subsequently reported processing complete. The build has not yet been
installed through TestFlight. In App Store Connect, the same export compliance
classification used for prior candidates (no non-exempt encryption) was saved,
the build reached Ready to Submit, and it was assigned only to the existing
internal `Release Validation` group with one tester. The test notes identify
this as an XPC broker validation build and exclude it from customer release.
App Store Connect then showed build 25005 as Testing in that group with one
invitation. The local TestFlight app still listed builds 25001–25004 after
refresh and restart, so the Apple-distributed 25005 payload could not yet be
installed. The manually bootstrapped 25004 broker job was removed and
`launchctl print` confirmed the label absent, preventing a stale broker from
masking the eventual 25005 runtime result. Installed Apple-re-signed
signatures and the product XPC runtime must be checked on build 25005 before
claiming this validation complete.
