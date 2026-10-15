# Issue #473: Windows cold-open restoration regression

Test date: 2026-10-15 (JST). Related: #473, shared fix #474, release
qualification #472.

## Implementation and baseline

The shared CLI startup fix at `2c50c8badbf4710fd4b9bc351ab18e5317636c2e`
(#474, including the deadline recheck from `b3019cd`) also fixes the Windows
restoration race. The automatic launch path polls Status while `opening` is
true, then submits Open once. Startup readiness retains its 15-second deadline;
an in-flight status query retains the normal IPC response timeout. A genuine
file-dialog BUSY or uncertain mutation outcome does not trigger a retry.

The Windows additions provide a native Named Pipe process regression and add it
to Build Binaries' existing cross-platform control-test command. They do not add
a second Windows-specific retry path, change the protocol, extend production
timeouts, or relax the pipe security boundary.

Before the fix, matching debug binaries from main
`7c1f49b2e9886ca0a66fa5cb5b1147d5fbe5b4c7` returned BUSY/exit 8 and retained the
wrong saved PDF in three of five cold attempts. One attempt succeeded, and one
returned IPC_FAILURE with no GUI remaining on a later probe; the latter's cause
was unconfirmed. This baseline is recorded in #472.

The new deterministic Windows process regression also fails against the saved
pre-fix `qp.exe`: its delayed-restoration scenario returns BUSY/exit 8 instead
of success. With the shared fix it passes all six scenarios below.

## Native Windows process regression

`tests/control_launch_windows.rs` copies the built `qp.exe` beside a disposable
compiled companion, with Japanese characters and spaces in both the executable
directory and requested relative PDF path. The working directory is outside
the executable directory. The companion uses the real Windows `ControlServer`
and machine-wide Named Pipe; it needs neither Python nor Slint nor PDFium.

| Scenario | Required result |
| --- | --- |
| Delayed saved-PDF restoration | Four Status requests precede exactly one Open; requested absolute path is returned; a second Open reuses the same GUI. |
| Immediately ready startup | One Status precedes one Open; companion reuse still works. |
| Restoration never finishes | TIMEOUT/exit 9 within the bounded budget; only Status requests, no Open. |
| File dialog busy | BUSY/exit 8 after one Open, including a second invocation against the running peer; no automatic mutation replay. |
| Disconnect after receiving Open | IPC_FAILURE/exit 7; exactly one Open and no replay after its uncertain outcome. |
| Ready Status arrives after startup deadline | TIMEOUT/exit 9; no Open submitted. |

The test refuses to run over an existing Quick Presenter endpoint. Close the GUI
before running it locally. Its fixture environment redirects the startup lock
to a private temporary directory. Output is captured in files so inherited
Windows pipe handles cannot delay waiting for the CLI. Cleanup verifies the
companion image path and retains its process handle before terminating it.

For the before/after reproduction, `QP_STARTUP_TEST_CLI` can select an older
CLI binary for this test only. Without it, the test always uses Cargo's freshly
built CLI. The pre-fix test failure is expected evidence, not a failure of the
final test suite.

## Real GUI/CLI cold starts

- Windows x64, 25H2, build 26200.9711; application 2.0.0 / Control Protocol v1.
- Matching locally built debug GUI/CLI from the shared-fix tree; local PDFium
  156.0.8076.0 configured via its DLL file as a development override.
- Debug GUI SHA-256: `95aababcc6928da905631454482b9a248d3aadd86e837f456020eb64afa6b108`.
- Debug CLI SHA-256: `230f390df6a89694cd47735f7e77752c5f375e76adaed6afa9bb839df4a01a66`.
- One observed 1920 x 1080 display at 125%; no display acceptance is claimed.
- Each attempt used a fresh private `APPDATA`. Normal user settings and the
  installed Store package were preserved.
- Saved deck A: the 12-page `docs/samples/quick-presenter-demo.pdf`. Requested
  deck B: a copy of the 3-page Marp fixture named `requested slides 日本語.pdf`.
- Before every attempt, Status had to report NOT_RUNNING. The test invoked the
  actual matching CLI from an unrelated directory and recorded its exit code,
  stdout/stderr, requested path, and final Status. It never retried an Open
  after a timeout. The test-owned GUI was terminated after observation; this is
  a startup test with seeded valid records, not a normal-quit persistence test.

| Saved state | Open exit/result | Final ready state |
| --- | --- | --- |
| Different valid saved PDF | 10/10 success | 10/10 requested B, page 1, rendering and notes ready |
| No saved record | 1/3 success; 2/3 IPC request TIMEOUT | 3/3 requested B, page 1, rendering and notes ready |
| Missing saved PDF | 1/1 success | 1/1 requested B, page 1, rendering and notes ready |

No attempt in this fixed-build matrix returned BUSY or left the saved PDF
displayed. The first successful saved-deck attempt also opened B again using
the same GUI session. That confirms the #473 restoration-race fix on this
Windows development setup; it does not qualify every startup workflow.

### Remaining first-open timing observation

Two no-saved-record attempts returned the ordinary IPC request TIMEOUT/exit 9
after approximately 31 seconds. An independent Status query then showed B fully
ready. The successful no-saved-record repeat took about 28.5 seconds. The saved
deck matrix completed Open in approximately 7.8-9.9 seconds; the missing-deck
attempt took approximately 4.7 seconds.

These are real debug-build observations, not discarded harness failures. They
are distinct from #473's BUSY/wrong-deck restoration race. Their cause has not
been established, and no production timeout was increased to conceal them.
Investigate first-open timing and response completion on the final release
build/Store installation before qualifying Windows cold startup. Preserve the
uncertain-outcome rule: query Status after TIMEOUT rather than replaying Open.

## Verification and release limits

- `cargo fmt --check`: passed.
- `cargo check --locked`: passed.
- `cargo build --bins --locked`: passed.
- Focused Windows process regression: passed all six scenarios.
- `cargo test --locked -- --test-threads=1`: **465 passed, 0 failed**, including
  the six-scenario Windows startup process regression and all fourteen
  renderer-process recovery/security tests; exit 0.

This is a development-build validation, not an installed MSIX/Store identity,
execution alias, signing/certification, physical keyboard, or final package
qualification. The final Store-installed GUI and `qp` alias must repeat these
startup checks without development PDFium overrides; #472 retains those gates.

Raw per-attempt JSON, command output and the real-GUI validation script are
retained locally under `artifacts/issue473-windows/` (ignored build evidence),
alongside the pre-fix CLI used for the negative regression. This document and
the committed process test provide the durable procedure and findings.
