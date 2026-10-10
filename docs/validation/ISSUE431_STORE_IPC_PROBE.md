# Store IPC probe validation

Date: 2026-10-10 JST. Scope: local Developer ID-signed sandbox experiment for
[#431](https://github.com/koichiro/quick-presenter/issues/431), before design
[PR #434](https://github.com/koichiro/quick-presenter/pull/434) is merged.

## Tested identity

| Item | Value |
| --- | --- |
| Implementation source | `b94336fca49af2b113efa4feb343cd9937a98dc1` |
| Branch and base | `codex/431-store-cli-ipc-probe`, based on `v2.5.0` at `32cec2b` |
| OS | macOS 27.0.1, build 26A434, Apple Silicon |
| SDK | macOS 27.0 |
| Rust | rustc 1.99.0, `b940084d7` |
| Probe build | `cargo build --locked --offline --example store_ipc_probe`, debug |
| Input binary SHA-256 | `e8a62bdf70c8c906b00238adc90c68e418040596a67b341828b674b9f3021815` |
| Signature | Existing Developer ID Application identity; runtime option, no signing timestamp, no notarization |
| Group | `<same validated signing team>.app.quickpresenter.control-probe` |
| Successful layout | A dedicated directory under `/Applications`, containing `Probe.app` and three nested helper apps |

The recorded hash identifies the unsigned/ad-hoc Cargo input copied into the
four signed executables. It is not the hash of the subsequently signed app or
an Apple-produced package. The source revision identifies the tested code;
the validation record itself is added in a later documentation commit. No
production app, production group, release package, or Store submission was
modified. Private certificate/profile files and raw logs remain outside Git.

## Automated checks

- `cargo fmt --check`: pass.
- `cargo check --locked --offline`: pass.
- `cargo build --locked --offline --example store_ipc_probe`: pass.
- `cargo test --locked --offline`: pass with the existing pinned PDFium library
  supplied by full file path through `PDFIUM_DYNAMIC_LIB_PATH`. Socket tests
  ran outside the agent filesystem/process sandbox. The seven new example
  tests passed alongside the existing unit/integration tests.
- Shell syntax and Python compilation checks: pass.
- Staged diff whitespace checks: pass.

The first restricted test run could not bind test sockets and lacked PDFium.
Using a directory instead of the library file for `PDFIUM_DYNAMIC_LIB_PATH`
also failed; the recorded successful run used the actual dylib. An existing
`fetch_update` deprecation warning in `src/macos_renderer.rs` and toolchain
deployment-version linker warnings remain unrelated to this change.

## Signed runtime results

The final nested bundle's `codesign --verify --deep --strict` check passed.
`check_macos_store_ipc_probe.py` exited 0 against the installed-layout package
and reported these eleven successful cases:

| Case | Observed result |
| --- | --- |
| Missing probe group | Self checks rejected the signed helper before endpoint access; no stdout. |
| Raw Cargo input | Ad-hoc/no-team self checks rejected it; no stdout. |
| Group/container and listener | Signed server resolved its group, created the private endpoint, and reported listening. |
| Mutual identity and roundtrip | Signed server/client passed peer checks and returned one expected diagnostic reply. |
| Continuous connection | Three ordered diagnostic replies arrived two seconds apart with clean NDJSON and reply ACKs. |
| Symlink invocation | A symlink to the signed nested client completed a roundtrip. |
| Wrong client identifier | Same-team, same-group, trusted signed helper with a different identifier was rejected; no reply payload. |
| Second server | A second server could not acquire the ownership lock or replace the active listener. |
| Concurrent commands | Eight single-reply requests with four concurrent callers succeeded. |
| Stale endpoint | After the runner terminated its server, a replacement recovered the stale socket and completed a roundtrip. |
| Wrong server identifier | Client rejected a same-team, same-group, trusted signed server with a different identifier; no reply payload. |

These are small diagnostic messages, not GUI presentation operations or a
Protocol v1 watch. They demonstrate the observed configuration and check paths,
not a race-free identity contract or Store-installed behavior.

## Findings that affect the design

### Bundle placement and sandbox initialization

Separate sandboxed apps staged under `/private/tmp`, and a nested-app layout in
that location, resolved the App Group and bound a socket. The client failed
`SecCodeCopyGuestWithAttributes` at probe stage 9 with OSStatus `100001`.
Filtered Sandbox logs included denied reads of the peer executable and bundle.
The final nested layout under `/Applications` passed the dynamic guest checks
without adding file-access exceptions. Install location is therefore part of
the signed proof and must be checked in #433; a staging-path failure is not
evidence that all installed layouts fail.

A trial with separately signed loose sandboxed client executables sharing the
outer bundle also failed before probe code, with a `libsecinit` SIGTRAP on this
OS. The final fixture gives each helper its own nested bundle and matching
signing identifier. This does not prove a loose Store `qp` layout is universally
unsupported; its independent initialization/provisioning is still a #433 gate.

### Frame deadlines

The initial client used a short `SO_RCVTIMEO` setting through Rust's socket API.
In the signed installed layout, the subsecond timeout setup returned `EINVAL`.
The final probe uses nonblocking socket reads/writes and `poll` against one
total deadline per frame. This workaround passed the signed roundtrip and
partial-frame deadline unit test; it does not change production transport IO.

### Peer lifetime and reply authentication

After an initial one-reply server closed its connection, the client could no
longer obtain its peer token for post-read revalidation. The final private
diagnostic protocol acknowledges each reply, keeping the server alive until
that verification is complete. The ACK is deliberately not added to Protocol
v1. #434 needs to account for this peer-lifetime issue when specifying when
server identity is established and how replies are trusted, or select a
transport with per-message peer identity such as XPC. Positive ACK-based
roundtrips do not settle the PID/exec/FD race questions.

## Remaining acceptance gates

macOS 14.4, Intel builds, Apple-re-signed TestFlight/Store installations,
foreign-team peers, adversarial PID reuse/exec/descriptor transfer, long
container paths, client/watcher saturation, slow readers/resource growth,
Security-call latency and shutdown, and actual GUI/Protocol v1 integration
remain unqualified. The four-watcher/eight-worker limits are implemented but
were not saturated in the signed runtime run; watcher-slot cleanup is covered
by a unit test. The normal signing/OS trust boundary is not replaced by these
tests.

Keep #431 and #422 open. This PR supplies an executable experiment and concrete
evidence for the #434 review; it does not approve the socket adoption decision
or complete #432/#433.
