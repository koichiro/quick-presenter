# Issue 431: queued request identity across exec

Date: 2026-10-20. Probe source before this addition:
`cc307caf2b5ec42d57ef96e8f9115e91fa7f8883` (PR #440). The installed
TestFlight candidate was built from
`f73794ac30aca6a336d42b6714d195af6db697c4`, version 1.9.0 (19001),
and installed through TestFlight. The server and approved Client were not
modified or re-signed for this test.

## Decision

The selected App Group socket authentication fails #434's identity gate.
`LOCAL_PEERTOKEN` identifies the current image associated with the socket PID,
not the code image that queued an individual request. The signed sandboxed
wrong-identifier fixture sent a request while the diagnostic server was
paused. Its original PID then executed the approved Apple-signed Client image.
The server accepted the already queued wrong-code request and returned the
exact diagnostic response. A direct negative control with the same fixture
and request, without exec, received no response. This paired result was
observed in the initial run and five repetitions.

The source socket descriptor had `FD_CLOEXEC`. A forked holder kept only the
wrong-code request connection alive; the approved Client did not inherit it,
did not participate in sending the request, and ran its normal `client 30`
command on a separate connection. The holder waited until after authentication
and response dispatch to read the queued connection. The runner verified the
original PID had become the exact approved Client path before resuming its
own server process. Pausing fixes scheduling for reproducibility; it is not
a required capability of a normal socket peer.

This establishes failure of the **tested probe identity policy** under an
ordinary signed-app exec transition. It does not claim that every possible
Unix socket authentication scheme is impossible, nor that a production GUI
command or PDF data was accessed. The diagnostic message was
`{"probe":1,"sequence":0,"count":1}`. The required message-origin property
is absent, so the proposed policy must not be integrated into Store control.
Do not replace it with a same-user, PID-only, or shared-secret fallback.
The separate broker/anonymous-endpoint XPC proposal in #434 is the next
candidate and requires its own installed validation.

## Signed environment

- macOS 27.0.1 (26A434), arm64; SDK 27.0.
- Installed server: TestFlight Beta Distribution,
  `app.quickpresenter.ipc-probe.server`, Team `NP5Q6U6NT5`, SHA-256
  `387f7dba7ff43d199e1bdc9579cb24730ca38726bbf878c339d6a0b411ce2e84`.
- Approved Client: TestFlight Beta Distribution,
  `app.quickpresenter.ipc-probe.client`, same team, SHA-256
  `59e70dcdb14bd09439f9dbcfef1b0487e4c39303182c81df7a347273b326c10c`.
- Wrong-identifier fixture: Developer ID Application,
  `app.quickpresenter.ipc-probe.exec-sandboxedwrong`, same team,
  App Sandbox and exactly the probe group
  `NP5Q6U6NT5.app.quickpresenter.control-probe`, SHA-256
  `064dacabf59b0578bfbfc362d6873a6726691f6ccf4f5d00116fb84095470f53`.
- `codesign --verify --strict` passed for the fixture and installed Client.
  Their code signatures and entitlements were inspected independently. The
  existing TestFlight record contains the installed server's receipt and
  full package verification.

## Reproduction

Use a fresh `/Applications` destination and the existing Developer ID
Application identity. No installed TestFlight bytes are changed:

```sh
bash scripts/package_macos_store_ipc_exec_probe.sh \
  /Applications/QuickPresenterIPCExecProbe-NEW \
  examples/store_ipc_probe/peer_exec_semantics.m \
  'Developer ID Application: Account Name (TEAMID)' TEAMID
python3 scripts/check_macos_store_ipc_exec_probe.py \
  '/Applications/Quick Presenter.app' \
  /Applications/QuickPresenterIPCExecProbe-NEW
```

The runner owns and stops its diagnostic server and fixture processes. It
checks the direct negative control before the exec case and emits a JSON
`adoption_gate: failed` only when the queued request receives the exact
diagnostic response. The first run and five repetitions produced that result.
No diagnostic process remained after the runs. No stack trace was generated
by these runs; the earlier `libsecinit` SIGTRAP concerned a different loose
helper layout and is not used as evidence for this decision.
