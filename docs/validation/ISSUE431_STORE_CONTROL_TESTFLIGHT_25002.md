# Issue 431: product Store CLI on TestFlight build 25002

Date: 2026-10-20. PR #496 contains the product XPC implementation from
`f16e44e` and the Store packaging correction from `5b62d5b`. This check used
version 2.5.0, build 25002, on macOS 27.0.1 arm64. It tested the product
GUI, CLI, and broker after Apple's TestFlight re-signing, rather than the
diagnostic components from build 25001.

## Distribution gate

`scripts/package_macos_app_store.sh` produced a signed package with SHA-256
`d8aa28424e44339ff4452772d8fbe54c6ea0d0e5386d27fbf8688927f6c20a8a`.
Deep strict app signature verification, installer signature verification, and
the six-Mach-O private API scan passed before upload. Transporter delivered
the package to App Store Connect. Build ID
`08bb347a-f313-4054-83a0-fb8e000c6153` reached `VALID` processing state.
Its export compliance field was set to `usesNonExemptEncryption: false`, the
same declaration as build 25001, after confirming the product XPC change adds
no cryptographic implementation. The build was added to the existing internal
`Release Validation` group only. No external beta review or customer release
was requested.

TestFlight installed `/Applications/Quick Presenter.app` as build 25002. The
app has a TestFlight receipt. `codesign --verify --deep --strict` passed for
the root app and both product control helpers. All three were re-signed with
`TestFlight Beta Distribution`, belong to team `NP5Q6U6NT5`, have App Sandbox
enabled, and share the exact
`NP5Q6U6NT5.app.quickpresenter.control` App Group. The signing identifiers
are `app.quickpresenter.QuickPresenter`, `app.quickpresenter.qp`, and
`app.quickpresenter.control.broker`. Installed executable hashes are:

| Role | SHA-256 |
| --- | --- |
| GUI | `b756ed315b7e532c9219bbe7f6f4596a958928bb8d7a5f666a137b513d206685` |
| CLI | `3a2452ccc77a13cdd04a86b681f6f5a9122c677e5c5f57881f91bae1d5fb2c6b` |
| Broker | `b914e8f11b882fe0aa11b136998be374c8a8ce4772bb39abedf99bbdcecbfdf2` |

## Installed runtime gate

The TestFlight GUI opened `docs/samples/quick-presenter-demo.pdf` and showed
page 1 of 12 with its notes and next-slide preview. With that GUI running,
the installed CLI passed the product integration runner:

```sh
python3 scripts/check_macos_store_control.py \
  '/Applications/Quick Presenter.app' --testflight
```

The runner verified `status`, elapsed timer, notes, slide, context, full-size
slide/context queries, all-page export, `next`, `goto`, blackout on/off,
`watch` snapshot and page-change events, `close`, and 32 repeated connections.
It also confirmed that raw-path `qp open` reports `UNSUPPORTED_PLATFORM` and
that no wrong-peer development fixture is included in the installed app.
After the GUI quit, `qp status --json` returned `NOT_RUNNING` with exit code
3. Starting the app again from TestFlight registered a new endpoint and
`qp status --json` succeeded. This validates product Protocol v1 over XPC in
one Apple-distributed build on this machine.

## Remaining gates

This does not establish App Store review acceptance or macOS 14.4/Intel
runtime behavior. Clean-account registration and permission flows, upgrades
from a customer Store release, foreign-team and unsigned clients against the
Apple-distributed broker, slow clients, and presentation-length load remain
unverified. Raw-path `qp open` still needs the authorized file handoff in
#432. Reassess export compliance if later builds add cryptographic behavior.

The subsequent CodeRabbit fixes to XPC connection cleanup, broker event
serialization, and absent-broker error mapping were not in build 25002. They
passed local compilation and tests. Their follow-up distribution validation
is recorded in `docs/validation/ISSUE431_STORE_CONTROL_TESTFLIGHT_25003.md`.
