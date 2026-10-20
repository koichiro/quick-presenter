# Issue 431: product Store CLI after XPC review fixes

Date: 2026-10-20. PR #496 source commit `729b57e5a328125385605e9ddd3c5eb7e150b54d`
was packaged as version 2.5.0, build 25003, for internal TestFlight testing on
macOS 27.0.1 arm64. This build includes the CodeRabbit fixes for inactive XPC
connection cleanup, broker handler serialization, and absent-broker error
mapping. It also includes the corrected product integration runner.

Transporter delivered the signed package with SHA-256
`14badd40725e41465c8a90841d14d4307b327d46c4232ff25f1af6ba6872ab3b`.
The pre-upload deep strict signature and installer signature checks passed, as
did the six-Mach-O private API scan. App Store Connect build ID
`cddb6120-1d59-4b7e-a9b3-1f347d3b14e5` reached `VALID`. The same
`usesNonExemptEncryption: false` declaration as build 25002 was applied, and
25003 was added only to the existing internal `Release Validation` group. No
external beta review or customer release was requested.

TestFlight updated `/Applications/Quick Presenter.app` from 25002 to 25003.
The installed app has a TestFlight receipt. Deep strict code signature checks
passed for the GUI, CLI, and broker. All three have `TestFlight Beta
Distribution` authority, team `NP5Q6U6NT5`, App Sandbox, and the exact
`NP5Q6U6NT5.app.quickpresenter.control` App Group. Their signing identifiers
remain `app.quickpresenter.QuickPresenter`, `app.quickpresenter.qp`, and
`app.quickpresenter.control.broker`. Installed executable hashes are:

| Role | SHA-256 |
| --- | --- |
| GUI | `b377b0b4acdddd26f7ec9b432225b232f88d0623695c4f753e77e6a237e8d0d6` |
| CLI | `3553fda9f124592fdc0838948992d4e1066fc65474c1e936de0749b5dccbea1b` |
| Broker | `c4cff9f44833d8221237e51f4e8b44ded01f90a8b1000362883a99d1953b30d5` |

With the GUI stopped, `qp status --json` returned `NOT_RUNNING` with exit code
3. The TestFlight GUI then opened `docs/samples/quick-presenter-demo.pdf` and
showed page 1 of 12 with speaker notes. The installed product integration
runner passed:

```sh
python3 scripts/check_macos_store_control.py \
  '/Applications/Quick Presenter.app' --testflight
```

It covered status, elapsed timer, notes, slide, context, full-size queries,
all-page export, watch snapshot and page-change events, navigation, blackout,
close, raw-path `open` rejection, and 32 repeated connections. A further 64
parallel `qp status --json` calls all succeeded against the running broker.
After GUI exit, `status` again returned `NOT_RUNNING` with exit code 3;
restarting from TestFlight registered a new endpoint and `status` succeeded.

The exact unregistered-agent error path and allocation-failure cleanup were
not forced in this installed test. Clean-account registration, upgrades from a
customer Store release, hostile clients against the Apple-distributed broker,
slow clients, presentation-length load, macOS 14.4/Intel runtime, and App
Store review remain separate gates. Raw-path `qp open` awaits the authorized
file handoff in #432.
