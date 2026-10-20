# Issue 431: TestFlight validation of the updated product broker

Date: 2026-10-20. PR #496 source commit
`22a1c3d1826a3932ea1a3191cb8f67423ac5ca4d` was packaged as version
2.5.0, build 25004, for internal TestFlight testing on macOS 27.0.1 arm64.
The signed installer SHA-256 was
`33e9845015d068cf99abcf0769a957b4aa4b4a8f6092e198cb2a4f2923f455ad`.
The package passed deep strict signature, installer-signature, and seven-Mach-O
private CGS import checks before Transporter uploaded it. App Store Connect
build ID `6649e764-bc6d-4816-acd9-38c3fc97692c` reached `VALID` with
`usesNonExemptEncryption: false`, and was assigned only to the existing
internal `Release Validation` group. No external beta review or customer
release was requested.

TestFlight updated `/Applications/Quick Presenter.app` from 25003 to 25004.
The installed app has a TestFlight receipt. Deep strict signature validation
passed, and the GUI, CLI, and broker carry `TestFlight Beta Distribution`
signatures, the expected identifiers, the same team, App Sandbox, and the
matching control App Group. Installed executable SHA-256 values are:

| Role | SHA-256 |
| --- | --- |
| GUI | `ccf32a72f8c1b61a837ad9d229ed1492e78ac2195f6e2f3e437b38303b4577c6` |
| CLI | `19e64abca6a53ff7ae3bc2e44f6c254d4362418d2668fbe52c53af9ae427e4e8` |
| Broker | `a88dce09f4e5fffce7c1a10541ae969bf13b3d39c37c810ef62e422bc756db64` |

The installed broker hash differs from build 25003, and `launchctl` showed
the running executable under the installed 25004 TestFlight app. With the GUI
opened on `docs/samples/quick-presenter-demo.pdf`, CLI status reported page 1
of 12 with rendering and notes ready. The installed product integration check
passed all seven groups: status, read-only queries, raw-path `open` rejection,
watch/navigation, goto/blackout, close, and 32 repeated connections. Another
64 parallel status calls passed. Closing the GUI returned `NOT_RUNNING` with
exit code 3; reopening it registered a new session and restored CLI status.
The sample PDF was closed and the GUI exited after the run.

This Mac also had an earlier development broker LaunchAgent with the same
Mach service names. Its stale service competed with the installed build, so
the old development agent was removed and the installed TestFlight broker was
manually bootstrapped from a temporary LaunchAgent for this validation. The
manual bootstrap qualifies the installed broker and IPC path, but does not
qualify unattended LaunchAgent registration or upgrade on a clean user
account. After an earlier managed service was manually booted out, the GUI
restart did not restore a launchd service; whether ServiceManagement reported
an enabled status or registration failed was not directly observed. A separate
clean-account test is needed. The exact unregistered-agent error path,
customer Store installation, macOS 14.4/Intel, and sustained presentation load
remain open validation gates.
