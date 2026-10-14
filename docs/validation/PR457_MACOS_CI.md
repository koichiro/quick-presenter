# PR457 macOS CI validation fixes

The initial macOS job in [CI run 38338776438](https://github.com/koichiro/quick-presenter/actions/runs/38338776438)
failed five GUI checks (112 passed). All A7 comment checks passed.

- Three notes checks assumed mixed-language fixtures fit particular font sizes
  regardless of the native viewport. Before PDF open, the small runner viewport
  legitimately selected 17 px for the short fixture and 12 px for the eight-line
  wrapped fixture (495 px measured height at 12 px versus a 180 px viewport).
  Short and multi-line fixtures now isolate vertical fitting on narrow displays;
  assertions still require 24 px for fitting short text, an intermediate size for
  multi-line text, minimum-size scrolling for long text, and maximal measured fit.
- A reaction expired during snapshot/resize observation: 2.90 seconds elapsed
  after the first frame while production lifetime is 1.8 seconds. GUI smoke now
  uses six seconds on all desktop platforms. Production lifetime is unchanged;
  motion, bounds, expiry, and stale-input checks remain enforced.
- The export check compared full status immediately after asynchronous navigation.
  It now waits for render and notes readiness before taking the pre-export status,
  preserving full before/after equality and reporting both statuses on failure.
- Notes-layout waits own their timer and wait for their own completion. An early
  loop exit cannot leave a delayed quit active for a subsequent check.

The separate main-targeted PR458 addresses macOS startup-restoration window-action
waits. These changes address the distinct failures in PR457's uploaded report.

Local macOS verification after these changes: `cargo fmt --check`, `cargo check`,
and `cargo test` passed (510 passed, 1 existing ignored test). The full GUI smoke
passed 246 checks with zero failures, including all five CI failure sites and all
A7 comment checks. GitHub-hosted runner results are tracked on PR457.
