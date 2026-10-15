# Issue 473: Cold CLI Open During Startup Restoration

## Scope

- Issue: #473; release qualification: #472.
- Date: 2026-10-15 (JST).
- Base: `7ff127f2b570ebaee4354195ae6d39831271224d`, application 2.0.0,
  Control Protocol v1.
- Platform: macOS 27.0.1 (26A434), arm64.
- Tested distribution: matching raw debug GUI/CLI binaries, not a signed DMG.
- PDFium: existing local development installation, 156.0.8076.0.

## Regression and fix

The new process-level restoration fixture returned `opening: true` for three
status queries and rejected any premature Open with `BUSY`. Before the fix,
`cold_open_waits_for_startup_restoration_before_sending_once` failed with that
error. After the fix, it observed four status queries and exactly one Open.

Only the CLI's automatic GUI startup path waits for `opening: false`. A ready
endpoint alone is insufficient while startup restoration is in progress.
Waiting retains the existing 15-second startup deadline and does not replay
Open. Already-running GUI requests retain their existing error behavior.

Process tests also verify that stalled restoration times out without Open,
file-dialog BUSY responses are not replayed on cold or subsequent requests,
and simultaneous startup requests launch only one companion while restoration
is pending. Existing no-restoration startup and open-failure tests still pass.

## Validation

- `cargo fmt --check`: passed.
- `cargo check --locked`: passed.
- `cargo test --locked`: **488 passed, 0 failed**, including all 10 startup
  process tests. The suite used the local PDFium path and permission to exercise
  local IPC. The existing `fetch_update` deprecation warning remains.
- Real Mac cold starts: **10/10 passed** with a valid saved PDF and a new private
  runtime directory per attempt. Each `qp open` returned exit 0; subsequent
  status identified `tests/fixtures/latex-beamer.pdf`, page 1 of 2, with rendering
  and notes ready. The remembered deck was a different sample PDF.
- Time from CLI invocation to ready status ranged from 0.667 to 1.752 seconds.
- Each test GUI was terminated after inspection. The startup-state file was
  verified byte-for-byte unchanged after the ten runs.

The production signed/notarized Developer ID package was not available for this
run. Repeat cold-start verification with the exact final package as part of
#472; this development-build result does not waive that gate.

## PR 474 review follow-up: late readiness response

CodeRabbit identified that a status request can start before the startup deadline
and return ready after it. The original code then submitted Open anyway.
A process fixture delaying its first status response by 16 seconds reproduced
this: the new regression expected TIMEOUT but received exit 0 before the fix.

The ready-status branch now checks the deadline immediately before submitting
Open. The regression verifies exit 9, typed TIMEOUT, empty stdout, and exactly
one status request with no Open. The normal IPC response timeout still applies
to an in-flight query; the readiness deadline prevents a subsequent mutation,
not an interruption of that query.

Formatting and Cargo check passed. All 11 startup process tests passed.
The complete test run had one failure in the unchanged renderer test
`stalled_broker_input_preserves_active_pdf_and_allows_shutdown` with
`valid deck did not open`; all 13 renderer tests passed on an isolated repeat.
The remaining slide-window layout test passed as well. Across those runs,
all 489 tests passed at least once; the initial full run was not clean.
