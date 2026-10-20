# Issue 431: signed local XPC rendezvous

Date: 2026-10-20. Source branch: `codex/431-store-cli-xpc-probe`, based on
`39555da4a274b2de1b48e3bce79e5681356e7460` from #440. The local package
was built on macOS 27.0.1 (26A434), arm64, SDK 27.0. The four diagnostic
roles were independently Developer ID Application-signed by Team
`NP5Q6U6NT5`, sandboxed, and entitled only to
`NP5Q6U6NT5.app.quickpresenter.control-xpc-probe` for control IPC. The
LaunchAgent was registered with `SMAppService` from the signed GUI role.

The local runner passed ten cases:

1. The unregistered service rejected the Client.
2. Registration reached `SMAppServiceStatusEnabled`.
3. The GUI registered an anonymous endpoint with the broker.
4. The independently launched Client completed three direct roundtrips.
5. A same-team/group wrong-identifier Client was rejected.
6. A same-team/group wrong-identifier GUI registration was rejected.
7. A second GUI registration was rejected.
8. The broker rejected lookup after the GUI exited.
9. A restarted GUI registered a new working endpoint.
10. The diagnostic LaunchAgent was unregistered after the run.

This is positive evidence for a per-user XPC rendezvous, exact identifier
checks, and endpoint lifecycle in this Developer ID-signed configuration. A
TestFlight diagnostic candidate, version 1.9.0 build 19002, was prepared from
the same native source and signed locally with Apple Distribution. Its signed
installer SHA-256 is
`2e13bd3a45f6487fd13a4d1da7a433ce6c8abf7aca3e9948394c524677670f89`;
the unsigned XPC probe input SHA-256 is
`2b44fc85823a7cda111681c72bcd691b33066bf812d4ca2bca4941cd0f8ff110`.
The original production GUI input SHA-256 is
`32f404c5bce62db6b5eb900a1db7b2940a2675fca4eea2e122f0f6835d8459b6`.
The candidate was staged from this branch's source and existing previously
built GUI/CLI/probe binaries; the packaging manifest contains exact input
hashes. Apple accepted the build, assigned it to the one-person internal beta
group, and TestFlight installed it. The installed version, receipt, deep
signature, four Apple TestFlight signing authorities, exact identifiers, team,
and sandbox/group entitlements passed validation. Runtime then failed while
the GUI waited for broker registration. `launchctl print` showed a broker job
with `last exit code = 78: EX_CONFIG`; the launch log reported
`OS_REASON_CODESIGNING | Launch Constraint Violation`, with the job's cached
constraint requiring validation category 6 (Developer ID), while the installed
Broker had category 2 (TestFlight). This reused the local fixture's agent
label. A later launchd attempt also reported an unresolved relative
`BundleProgram`. Build 19002 therefore does not qualify the XPC route.

Build 19003 changes only the TestFlight LaunchAgent label to avoid the local
Developer ID fixture's cached launch constraint. Its signed installer SHA-256
is `e1ca28b4567d16ba1b9a3b766790b846ddabe2e8eb21bcd47eef2c4e8f26271f`.
The native XPC probe input is byte-for-byte identical to build 19002. Apple
accepted it, and the same one-person internal beta group installed it through
TestFlight. The installed build had a receipt, valid deep signature, four
TestFlight Beta Distribution authorities, the expected team and exact
identifiers, and sandbox/group entitlements. The verifier passed all ten
cases listed above on the installed Apple-re-signed payload. The four installed
helper executable SHA-256 hashes were:

| Role | SHA-256 |
| --- | --- |
| GUI | `bfc5345506f52e9109bd8458db8d275eeaab87b88d91cb6cc9370b9a3400b275` |
| Broker | `240527136930714bc633edd0866d8065bafbd853f3fa5504ba4f7996f954b744` |
| Client | `73201d5466de7a69a3e58c64757d4f3636d74991873ce946f17db29588a53ddb` |
| WrongPeer | `f4ba0dd7d9fa4616b0a2bede2bdce39b442984e1d4bf1fbe94ba1c851758e0f8` |

This closes the Apple-re-signed transport and endpoint-lifecycle gate for
this macOS 27 arm64 internal diagnostic. It does not validate production
control commands or a customer Store release. The 19002 failure and 19003
success together support the inference that reusing the agent label across
Developer ID and TestFlight signing categories caused the cached constraint
mismatch; the diagnostics should keep those labels separate.

Open gates: macOS 14.4 and Intel, real GUI/Protocol v1 integration, bounded
watch/full-text delivery, resource limits, user approval/denial on another
machine, final Store distribution, and #432 file-authority handoff. #431/#422
remain open.
