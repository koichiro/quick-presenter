# Issue #473: Linux cold-open restoration regression

Test date: 2026-10-15 (JST). Related: #473, shared fix #474, Ubuntu release
qualification #475.

## Change and source

The startup-wait implementation from #474 is common CLI code and also fixes the
Linux reproduction. This follow-up adds Linux package-layout coverage and
records real desktop regression results; it does not add another retry path.
The prerequisite is `b3019cd105c28927caaebdf763c04fea5a550aa9`, including the
startup deadline recheck, plus this PR's `tests/control_launch.rs` regression.

Before the shared fix, source `7ff127f2b570ebaee4354195ae6d39831271224d`
returned BUSY/exit 8 in 7 of 8 independent Ubuntu cold starts and left the saved
sample PDF displayed instead of the requested Beamer deck. The baseline
reproduction is recorded in
[the issue comment](https://github.com/koichiro/quick-presenter/issues/473#issuecomment-6184404933).

## Automated regression

`debian_symlink_cold_open_waits_for_restoration_and_reuses_packaged_gui` uses the
Debian layout (`usr/bin/qp -> ../lib/quick-presenter/qp`) with a disposable
companion GUI and a working directory outside the executable directories.
It opens a relative path containing spaces and Japanese characters while the
companion reports delayed startup restoration. It requires four status polls
before exactly one Open, the correctly resolved requested path, and exactly one
GUI launch across a second Open.

A separate before/after process reproduction used that layout and the same
restoration fixture with old and fixed CLI binaries. The old CLI sent one status
and one Open, returning BUSY/exit 8. The fixed CLI sent four statuses and one
Open, returning success/exit 0. The Linux full suite also exercises the shared
restoration timeout, late-ready deadline, genuine file-dialog BUSY without
replay, and concurrent startup tests from #474.

- `cargo fmt --check`: passed.
- `cargo check --locked -j 2`: passed.
- `cargo test --locked -j 2`: **492 passed, 0 failed**, including all 12
  `control_launch` process tests. IPC/renderer tests ran outside the command
  sandbox with the matching PDFium library configured.

## Real desktop matrix

- Ubuntu 26.04.1 LTS x86_64; kernel 7.0.0-38-generic; GNOME Wayland.
- One DELL U4021QW, 2560 x 1080 at approximately 60 Hz, scale 1.0.
- Native Wayland and XWayland inside the same desktop session; this is not a
  native Xorg test.
- Matching debug GUI/CLI in a locally built Debian package, extracted into a
  temporary directory. Both PDFium override variables were unset; bundled
  PDFium chromium/8076 was used. The system-installed package was preserved.

Each attempt used a fresh private runtime and configuration directory. For a
saved-deck attempt, the valid startup-state record names deck A, the 12-page
sample `docs/samples/quick-presenter-demo.pdf`. Deck B is a copy of
`tests/fixtures/latex-beamer.pdf` with spaces and Japanese characters in its
absolute filename. The missing-deck case names a nonexistent saved PDF; the
no-saved-deck case has no startup record.

The packaged `usr/bin/qp` symlink was invoked from `/tmp`. Before each open,
status returned NOT_RUNNING. After `qp open B --json`, the check required exit 0,
empty stderr, B in the mutation response, and status with B/page 1, opening
false, rendering ready, and notes ready. Exactly one companion GUI was running
for the private runtime. Validation GUIs were terminated after inspection; the
saved record was seeded again independently for the next attempt, so this is
not a normal-quit persistence test. User settings were unchanged.

| Backend | Different saved deck | No saved deck | Missing saved deck |
| --- | --- | --- | --- |
| Native Wayland | 10/10 passed | 1/1 passed | 1/1 passed |
| XWayland | 10/10 passed | 1/1 passed | 1/1 passed |

**24/24 cold starts passed.** Each requested Beamer deck reached ready rendering
and notes; no BUSY result or wrong saved deck remained in this matrix. These
results demonstrate the shared fix on the tested Linux setup; they do not
establish an exhaustive absence of timing races.

## Artifact identity and limits

Local debug validation package: `quick-presenter_2.0.0_amd64.deb`.

- Debian SHA-256: `5ce31c16ffe76ca1ba092c8f9bdd941918070f4e601e76d17ab438e8cee9d4da`
- GUI SHA-256: `0f52d8be9853757035c4cb7fc242b1e61efbc240d9d90774b6657ec9563bf9b5`
- CLI SHA-256: `d0f853a845fb898d90278041a49237e3cebd781dbfb05ed44243c69ecda71d3d`

These identify the local development artifact, not the final CI release package.
The final installed Debian package must still pass the release regression and
manual gates in #475. This run does not qualify macOS, Windows, other desktop
environments, native Xorg, physical keyboard/focus behavior or multiple displays.

Raw command logs, per-attempt JSON and temporary reproduction scripts remain
under `/tmp/qp-473-linux-evidence` for this session. They are not committed as
historical raw logs; the results and procedure above provide the durable record.
