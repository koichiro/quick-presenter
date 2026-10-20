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
assigned to the internal `Release Validation` group or installed through
TestFlight. At the time of this record, TestFlight still listed builds
25001–25004. Installed Apple-re-signed signatures and the product XPC runtime
must be checked on build 25005 before claiming this validation complete.
