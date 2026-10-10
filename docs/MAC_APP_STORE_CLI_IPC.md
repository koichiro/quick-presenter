# Mac App Store CLI IPC design

Issue: [#431](https://github.com/koichiro/quick-presenter/issues/431). Parent:
[#422](https://github.com/koichiro/quick-presenter/issues/422). Target: `v2.5.0`.
Baseline: `21d729b658c75caf98c01ca18ba5155182c07441` on `v2.5.0`.

## Proposed decision

Evaluate an App Group Unix domain socket first, retaining Presentation Control
Protocol v1 byte framing and the existing Rust server. Add mutual authentication
of the running GUI and bundled CLI before either process exchanges protocol
data. Adopt this transport only after the signed proof of concept demonstrates
the identity, sandbox, and bounded-resource gates below. App Group membership,
socket permissions, and a matching PID alone are insufficient authentication.

The socket identity proof is the largest uncertainty in #431. If public socket
and Security APIs cannot establish an adequate identity binding on every
supported Store OS, stop this approach and evaluate the XPC broker alternative.
Do not ship a weaker same-user socket as a fallback. This is a design proposal;
the signed and Apple-re-signed runtime gates remain open.

## Scope and ownership

The first usable increment controls a running Store GUI with a PDF selected
through that GUI: status, notes, slide/context including full text, elapsed time,
next/previous/goto, blackout, close, and watch. Timer access stays read-only.
The GUI remains the sole presentation-state owner and processes `PendingRequest`
on its event loop. PDFium remains in its existing isolated renderer.

[#432](https://github.com/koichiro/quick-presenter/issues/432) owns GUI startup
and file-authority handoff. Until that work lands, reject Store
`presentation.open` with the existing `UNSUPPORTED_PLATFORM` error before
dispatching to the ordinary arbitrary-path open pipeline. This is an explicit
intermediate limitation, not completed v2.5.0 support. An authenticated CLI path
does not confer file authority. Keep protocol parsing of `Open` intact so #432
can supply its authorized implementation.

[#433](https://github.com/koichiro/quick-presenter/issues/433) owns final package,
installation/PATH documentation, and TestFlight/Store-installed qualification.
#431 still requires a minimally packaged, signed sandboxed GUI/CLI pair to prove
its design; it cannot defer all signing and entitlement work to #433.

## Transport comparison

| Candidate | Benefits | Costs and decision |
| --- | --- | --- |
| Unix socket inside an App Group container | Apple documents this sandbox IPC route; preserves byte framing, bounded writes, watch, and existing server ownership. | Requires container access from a shell-launched CLI, short socket paths, and a separate reliable running-peer signature check. First PoC candidate, subject to identity gates. |
| App Group named XPC endpoint through a per-user launch agent | Native XPC same-team and signing-identifier requirements can validate every message, including Apple-re-signed code. | Requires a supported registered service, an additional broker/lifecycle, and setup or background-item authorization. Evaluate if the socket identity gate fails. |
| Existing nested renderer XPC service | Already has native same-team authentication. | Reject for control: it is a PDF isolation boundary, not a GUI control service. Do not give it presentation state or App Group access. |
| Existing `/tmp` socket, TCP, temporary sandbox exceptions, shared secret file | Small initial implementation change. | Reject: the old socket does not provide the Store authority model; TCP and broad exceptions expand permissions; a secret file does not prove peer code identity. |

Apple permits Unix sockets in the shared container and specifies address-length
limits in the [App Groups entitlement documentation](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.security.application-groups).
A named XPC listener is not automatically publishable by an ordinary GUI:
the SDK's `XPC_CONNECTION_MACH_SERVICE_LISTENER` contract requires an advertised
`launchd` service and forbids using the flag for dynamic registration.
[Apple DTS](https://developer.apple.com/forums/thread/818192) distinguishes a
GUI-hosted App Group socket from a launchd-hosted named XPC endpoint. App Group
membership grants IPC permission; it does not itself register a service.

## Endpoint discovery and ownership

Use one explicit macOS-style group identifier,
`<TEAMID>.app.quickpresenter.control`, generated from the release signing team.
Resolve its directory with
`NSFileManager.containerURLForSecurityApplicationGroupIdentifier`; do not
construct a path from `HOME` or a hard-coded `Library/Group Containers` prefix.
The [configuration documentation](https://developer.apple.com/documentation/xcode/configuring-app-groups)
describes the team-prefixed format and container lookup. It is a candidate
configuration, subject to signed CLI and installed-build validation.

The proposed relative endpoint is `c/s`, with ownership lock `c/l` and the future
#432 startup lock `c/start`. Create only `c` with mode 0700; do not change the
permissions of the system-managed container root. Socket and lock files use
0600. Check the actual address bytes, including the terminating NUL, against
the target SDK's `sockaddr_un.sun_path` capacity before binding or connecting.
Long home/container paths fail explicitly; never shorten them through a `/tmp`
symlink or silently switch transports. Include a long-home-path case in the PoC
before adopting this route for distribution.

Apply existing Unix validation to the dedicated `c` directory, not the group
root. Reject wrong ownership, symlinks, non-sockets, and unsafe permissions;
use directory-relative operations with no-follow checks where needed to avoid
check/use substitution. Hold the private ownership lock for the server lifetime.
Only the lock holder may remove a refused, verified stale socket. Preserve live
sockets, unexpected files, and sockets with an indeterminate probe result.
Shutdown removes only the device/inode owned by that server. A second GUI may
continue playback but does not take over control. Endpoint failure logs a short
diagnostic and leaves GUI playback available.

Store GUI/CLI select this route from validated signed configuration and their
own entitlements, with an exact group match. Developer ID/development builds
retain the current Unix route and Windows/Linux retain their existing routes.
Store commands ignore `XDG_RUNTIME_DIR`; no environment or user-supplied path
can downgrade Store authentication or redirect to a Developer ID instance.
Missing/invalid Store configuration is an error, not a reason to use `/tmp`.
Help/version queries remain independent of endpoint discovery and PDFium.

## Mutual peer authentication

### Identity policy

Both peers require the same effective user, a valid trusted signature, the
expected signing identifier, and the same nonempty developer team as their own
validated signature. The GUI accepts only the bundled Store CLI identity
`app.quickpresenter.qp`; the CLI accepts only
`app.quickpresenter.QuickPresenter`. The CLI identifier is a proposed new signing
identifier to set explicitly in the signing step and verify in the PoC.
Also verify the exact control-group entitlement and independent App Sandbox
entitlement on both peers. Same-team helpers with another identifier are denied.

This supports shell automation by invoking the signed bundled `qp`, including
a symlink to it. It does not authorize arbitrary raw protocol clients or an
unsigned rebuilt CLI against the Store GUI. Developer ID/development automation
retains its existing policy. Matching identities authorize any invocation of
the approved CLI by that user; this is not human approval of each command or a
defense against a compromised approved executable.

### Socket proof of concept

The candidate native bridge obtains a kernel-reported peer audit token using
`getsockopt(SOL_LOCAL, LOCAL_PEERTOKEN)` and binds a dynamic `SecCode` object via
`SecCodeCopyGuestWithAttributes` with `kSecGuestAttributeAudit`. Validate that
running code with `SecCodeCheckValidity`, including an Apple-trusted anchor and
the exact identifier requirement, then inspect validated signing information
for the team and entitlements. Reject ad-hoc, self-signed/untrusted, missing-team,
and wrong-identifier code. Validate self before deriving the expected team.
Never use a peer-reported token/team, executable filename, reopened bundle path,
PID-only lookup, or certificate leaf OU as an Apple-re-signing identity policy.
[Security signing information](https://developer.apple.com/documentation/security/signing-information-dictionary-keys)
exposes team and identifier fields; their presence alone is not validation.

**Do not assume this provides XPC-equivalent message authentication.** The
installed SDK exposes `LOCAL_PEERTOKEN`, but Apple's current
[XNU socket implementation](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/uipc_usrreq.c)
derives it from the peer socket's last PID and a task lookup. It is not an audit
token attached to each protocol frame. PID reuse, exec, descriptor transfer,
and the interval between identity checks and reads/writes require explicit
analysis and adversarial tests. Reading the token twice or comparing PID values
does not by itself close these races. Public source behavior is not a promise
about every supported OS version.

The PoC must establish which running process is authenticated on accept/connect,
after a complete request frame, and before each response/watch write. It must
show that a rejected process cannot substitute itself or obtain presentation
data through inherited/transferred descriptors. Use close-on-exec descriptors;
the product does not fork or transfer control sockets. If the public API contract
cannot support the required identity binding, record the failure and choose the
XPC redesign rather than treating ordinary positive tests as proof. Do not add
a UID-only or PID-only fallback when `LOCAL_PEERTOKEN` is unavailable.

The server authenticates before reading application payloads or allocating a
watch subscription, and verifies the accepted identity again before dispatch.
The CLI authenticates the server before sending any path, command, or query.
Untrusted connections close without a protocol response or state disclosure.
Authentication failures map locally to `IPC_FAILURE` / exit 7 and never trigger
GUI launch or command replay. Store authentication is isolated from the renderer
XPC implementation; do not change the renderer's identity policy to share code.

### XPC alternative if socket identity fails

Prototype a separate sandboxed per-user control broker registered through
Service Management, with an App Group-prefixed Mach service. No root daemon,
private bootstrap registration, or renderer reuse. The broker is a rendezvous
only: the GUI registers an anonymous XPC endpoint with it, and the CLI obtains
that endpoint and talks directly to the GUI. The broker stores no PDF data or
presentation state. GUI absence produces `NOT_RUNNING`; #432 owns activation.

Each broker/GUI/CLI connection installs exactly one
`xpc_connection_set_peer_team_identity_requirement` with the exact peer signing
identifier before activation. Check same-user/session routing independently;
same team alone does not identify the user. The native API checks each message
and handles the Store re-signing case; do not layer a second signing-requirement
setter on the same connection. See Apple's
[API contract](https://developer.apple.com/documentation/xpc/xpc_connection_set_peer_team_identity_requirement(_:_:)).
Retain macOS 14.4 as the Store minimum.

This alternative requires a revised implementation decision covering broker
registration/denial/removal, GUI registration ownership, endpoint invalidation,
and bounded XPC delivery/acknowledgement for watch and full-text responses.
XPC asynchronous send completion is not consumer backpressure. Keep the v1
public JSON/CLI contract and existing payload limits; identify any transport
envelope changes explicitly. Re-estimate #431/#433 before implementing this
alternative rather than silently adding a background service to the socket plan.

## Rust integration and limits

The socket candidate preserves this path:

```text
signed bundled qp
    -> Store endpoint discovery -> authenticate GUI -> framed request
    -> GUI socket worker -> authenticate CLI -> bounded PendingRequest
    -> GUI event loop -> existing presentation/session operations
    -> framed response or bounded watch -> qp formatting and stdout
```

| Boundary | Proposed responsibility |
| --- | --- |
| `src/control/transport.rs` | Select a typed endpoint configuration and peer policy; retain existing non-Store endpoints. |
| New macOS control bridge and Rust wrapper | Read self configuration, resolve the group container, obtain kernel peer identity, and validate running code. Return owned results and typed failures; release CF objects and keep native calls off the UI thread. |
| `src/control/server.rs` | Authenticate each accepted Store connection; apply endpoint policy, existing framing/deadlines, bounded dispatch, cancellation, and watch cleanup. |
| `src/control/client.rs` | Authenticate connected Store server before writing; reuse response assembly, event decoding, output, and exit mapping. |
| `src/control/launch.rs` | Return the intermediate unsupported Store-open result without the existing raw companion spawn; #432 supplies Store activation later. |
| `src/control_app.rs` | Preserve GUI-owned domain operations; enforce the temporary Store-open restriction before ordinary open dispatch. |
| `build.rs` and signing/staging scripts | Compile only the macOS bridge, configure exact CLI identity and minimal PoC entitlements, and validate resulting signed configuration. |

Use a small concrete endpoint/policy type rather than a general transport plugin
framework. Tests may inject policy facts and time/IO failures; production Store
configuration must not permit a bypass switch. Keep the native bridge limited
to OS identity/container operations. Do not put PDF or presentation logic in C,
Slint, the CLI, or a broker.

Retain the existing bounds: eight active client slots, eight pending requests,
four watchers, 64 queued events per watcher, 1 MiB frames, 32 MiB assembled
responses, a one-second frame read/write budget, a 30-second request deadline,
and two-second hidden watch heartbeats. Full-text safety limits and transfer
assembly remain unchanged. Count authentication-in-progress against client
capacity; never spawn unbounded validation tasks. Authentication time consumes
the connection deadline. Security calls may not be cancellable: use a fixed
worker budget, never block GUI/accept on validation, and gate adoption on measured
worst-case completion/shutdown behavior rather than promise cancellation the API
does not provide.

Watch authentication and identity-change detection must cover idle periods as
well as events, with measured CPU cost. Slow/blocked consumers retain existing
lag, timeout, and disconnect behavior. On EOF/shutdown/error, cancel the pending
request or subscriber and release the client slot and descriptors. Do not replay
commands after disconnect, automatically reconnect watch, or change snapshot
and sequence semantics. An uncertain mutation outcome still requires status
before a retry.

Keep protocol version 1 and existing error codes. Missing endpoint/refused stale
endpoint gives `NOT_RUNNING`; rejected signatures, unsafe paths, configuration,
permissions, or failed native checks give `IPC_FAILURE`; elapsed deadlines give
`TIMEOUT`. Single-response failure leaves stdout empty. Watch retains complete
emitted lines and puts its terminal error on stderr. Logs contain bounded,
redacted failure categories, not notes, PDF paths, tokens, or signing profiles.

## Entitlements and signing proof

| Component | Control-related entitlement proposal |
| --- | --- |
| Store GUI | Existing sandbox, user-selected read-only access, and app-scoped bookmarks, plus the exact control App Group. |
| Bundled Store `qp` | Independent App Sandbox plus exact control App Group. No inheritance, network, PDF access, or bookmark entitlements for #431. |
| Renderer proxy and XPC renderer | No new control App Group or control endpoint. Keep current isolation/signing contracts. |
| Optional XPC broker | Separate sandbox/App Group configuration and signing identifier only if the alternative is selected. |

Determine the CLI's supported code-bundle layout and provisioning context in
the PoC. Do not assume a loose Mach-O at `Contents/MacOS/qp` gains all container
authority from the parent GUI profile. Validate shell invocation and symlink
invocation from a terminal with its actual signed identity and entitlements.
If a small nested CLI bundle is necessary, #433 must preserve a documented
stable invocation path. Check the GUI profile, CLI authorization context,
group spelling, team, signed entitlements, and installed-container behavior.
The [container protection documentation](https://developer.apple.com/documentation/xcode/accessing-app-group-containers)
makes newer macOS container access an additional validation concern.

Use a dedicated signed sandboxed development package for the PoC and identify
it separately from an Apple Distribution submission artifact. A Developer ID
certificate may provide development evidence for the sandbox configuration;
it does not establish Apple-re-signed identity behavior or Store acceptance.
Neither a signing success nor a profile inspection closes installed-runtime
gates. No broad sandbox exceptions or network entitlement are added to make
the experiment pass.

## Validation and implementation sequence

1. **Container and signature probe.** Build a minimal signed sandboxed GUI/CLI
   pair; verify independent CLI launch, container lookup, bind/connect,
   address-length handling, trusted self/peer identity, and positive/negative
   peers on macOS 14.4 and a current supported macOS. Capture identity metadata
   separately from private signing files.
2. **Authentication review.** Examine public API guarantees and test PID reuse,
   exec, descriptor inheritance/transfer, server replacement, same-team wrong
   identifiers, and both impersonation directions. Accept no commands or source
   data before authentication. A missing adequate identity contract is a failed
   gate even if normal commands work. If necessary, select the XPC alternative
   and revise this design before control integration.
3. **Protocol integration.** Add Store endpoint/policy selection and mutual
   authentication to the existing bounded server/client path. Test a GUI-selected
   fixture PDF, queries/full text, navigation boundaries, blackout, close, and
   read-only timer behavior. Keep Store open/startup deferred to #432.
4. **Load and lifecycle checks.** Exercise eight clients, a ninth refused client,
   four watchers and a fifth refused watcher, queue pressure, slow readers,
   malformed/oversized frames, response chunking, watch idle/lag/disconnect,
   restart/reconnect snapshots, shutdown, stale sockets, and two GUI instances.
   Measure CPU, memory, descriptor/worker counts, latency, and bounded cleanup.
   Verify a failed listener/authentication path leaves playback responsive.
5. **Regression and evidence.** Test the unchanged Developer ID/unsigned
   development path, Windows pipe, and Linux Unix route. Run required Cargo
   checks for implementation changes. Record the selected transport and exact
   successful/failed signed tests; leave Apple-re-signed qualification open for
   #433 and keep #422 open until all child gates are met.

Pure Rust tests cover route selection/fail-closed behavior, identity policy facts,
deadline/error mapping, frame limits, queue pressure, cleanup decisions, and the
Store-open guard. Native signed integration tests validate the real OS boundary;
mocked signature facts are not a replacement. For Rust/Slint/build changes run
`cargo fmt --check`, `cargo check`, and `cargo test`. Coverage is not required
unless requested or CI requires it. This documentation-only design needs a
diff/link/consistency review, not Cargo execution.

Every runtime record identifies source commit, OS/architecture, SDK, GUI/CLI
signing identifiers and team/group configuration, package identity, test cases,
and results. Keep certificate material, profiles, raw tokens, private logs, and
personal paths outside Git. Store development evidence under `docs/validation/`
with a redacted summary. No signed runtime result is asserted by this proposal.

## Adoption gates and handoff

- Socket sandbox permission and shell-launched CLI container access work without
  exceptions on the supported Store OS range.
- Mutual running-code authentication has a defensible public API identity binding
  and rejects foreign, unsigned, untrusted, wrong-identifier, and raced peers.
- Concurrency, watcher/response backpressure, deadlines, and resource cleanup stay
  bounded and do not delay GUI playback.
- Existing protocol/CLI semantics and non-Store distributions remain intact.
- The exact signed PoC results determine the selected transport and updated
  estimate. #432 consumes its endpoint/identity contract; #433 consumes the
  validated CLI layout, entitlement, signing, and installation requirements.

If any gate fails, #431 stays open. Record the reason and revise the transport
decision; do not describe Store control as supported or downgrade authentication
to reach the milestone.
