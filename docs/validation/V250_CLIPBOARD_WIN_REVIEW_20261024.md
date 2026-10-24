# clipboard-win Windows impact and patch update — 2026-10-24

Tracking: [#508](https://github.com/koichiro/quick-presenter/issues/508).
This addresses the new warning found during #517's Linux qualification; it
is not a new comprehensive PDFium/CVE review or publication approval.

## Finding and bounded application impact

The previous lock selected **clipboard-win 5.4.1**, now yanked in the official
crates.io registry. Registry metadata was freshly checked on October 24:
**5.4.2** and **5.4.3** were available and not yanked. A yank flag alone does not
state the reason, establish a CVE or demonstrate exploitation in Quick Presenter.

The public [5.4.2 change](https://github.com/DoumanAsh/clipboard-win/commit/38c3c3fef175c20ed8f8e33dacb9b1e3bb8eca87)
replaces unchecked HTML-fragment pointer copying with checked string slicing
and adds a malformed-fragment regression case. In 5.4.1 a small fragment length
can satisfy the old size check while its start offset points beyond the buffer;
that permits an invalid native read in that API. The update also rejects invalid
UTF-8 boundaries through the checked slice. This is source-based analysis, not
a claim of a demonstrated application exploit or proof of the registry yank reason.

The shipped Windows normal dependency route is:

```text
Quick Presenter → Slint/backend selector → vendored i-slint-backend-winit
                → arboard 3.6.1 → clipboard-win
```

The inspected adapter in `vendor/i-slint-backend-winit/clipboard.rs` exposes only
`String` get/set operations and calls arboard `get_text`/`set_text`. The exact
arboard 3.6.1 Windows text implementation checks `CF_UNICODETEXT` and reads it
with `clipboard_win::raw::get_string`; writes use `raw::set_string`. Its HTML
getter is a distinct `html` operation invoking `raw::get_html`, which this
application adapter does not call. No direct application HTML clipboard call
was found. Consequently the identified HTML-fragment bug is not reached through
the inspected Quick Presenter text copy/paste route. This does not declare the
old crate generally safe or turn absence of an application reproduction into
a library security guarantee. Changes that enable HTML clipboard access require
this reachability assessment to be revisited.

## Selected correction

Update only Cargo.lock's `clipboard-win` entry **5.4.1 → 5.4.3** and its registry
checksum. arboard's existing `5.3.1` semver requirement accepts this version;
no manifest constraint, application/vendor source, feature or other dependency
was changed. No advisory ignore or risk exception was added.

The exact [5.4.3 upstream revision](https://github.com/DoumanAsh/clipboard-win/commit/ed27008ae5d1f10cfbb266de039f0fb35fd05a56)
is the direct child of the 5.4.2 fix. It additionally changes the library's
`new_attempts` retry sleep from 0 to 5 milliseconds. arboard's inspected text
path uses its own open/retry loop with `Clipboard::new` and an existing 5 ms
sleep. The broader version update is still checked on Windows below.

Official 5.4.3 crate SHA-256:
`052af7878466feccda12ba7a6a67d019d712f296809559b47c12c8cc196d3a9a`.
The freshly downloaded archive matched the registry checksum; source in that
archive contains the checked HTML slicing and identifies upstream
`ed27008ae5d1f10cfbb266de039f0fb35fd05a56`.

Target-filtered normal graph inspection after the update found **5.4.3 on Windows
MSVC x64** and no `clipboard-win` node on macOS arm64 or Linux x64. Those targets'
clipboard implementations do not use this Windows-only library. The global
lockfile/source identity nevertheless changes; preserved old release artifacts
and sidecars must not be relabeled as a new-source build.

## Validation

- Local macOS: `cargo fmt --check`, `cargo check --locked` and the full
  `cargo test --locked` passed. These are host checks, not Windows execution.
- Fresh `cargo audit --json`: **0 vulnerability advisories**, **no yank warning**,
  and the same five unmaintained warnings (`bincode`, `ttf-parser`, `zbus_names`,
  `zvariant`, `zvariant_derive`). Existing migration deferrals remain in force;
  `.cargo/audit.toml` keeps `ignore = []`.
- RustSec snapshot: **1351 records**, commit
  `84d943a6deeb8cc6c6f632531aaf4f1c62275239`.
- Windows execution and replacement-payload evidence are tracked separately
  in the dependency-update PR and #508. Read those results before using a new
  candidate; the macOS host checks above do not establish Windows execution.

## Final artifact boundary

This dependency change supersedes the Windows dependency content of selected
source `ce42e1d`. The prior Windows #516 execution remains valid evidence for
its exact old package, not a pass for a changed dependency or new package.
Build the final Windows submission input again after merge, regenerate and
fully verify its feature/target-matched SBOM, and record new source/package
hashes before release. Package CI and any local replacement payload are separate
from Microsoft Store customer signing/install/update/alias qualification.

Mac/Linux normal graphs do not contain this crate. Retain their earlier runtime
evidence within its exact artifact scope and verify/rebind new source/lockfile
inventory before calling a later artifact final. Do not overwrite the preserved
#513 package hashes or assume a docs-only source change. No package was published,
Store review submitted, or other #508 environment/authority/traceability/security
condition closed by this dependency review. Raw outputs remain outside Git.
