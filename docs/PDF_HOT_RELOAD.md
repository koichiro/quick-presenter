# PDF Hot Reload

## Status

This document defines the implementation design for
[GitHub issue #347](https://github.com/koichiro/quick-presenter/issues/347).

PDF hot reload is always active while a document is open. It is not a user
preference and does not add a menu item, setting, or keyboard shortcut.

## Goals

- Reload the active PDF after either an in-place write or an atomic
  rename/replace save.
- Preserve the current zero-based page index, clamping it to the new last page
  when necessary.
- Keep the last successfully loaded document visible until a replacement has
  been opened and its current page has been rendered successfully.
- Reject stale filesystem signals, reload candidates, and render results.
- Invalidate all caches and derived document data when a replacement is
  committed.
- Preserve fullscreen, window focus and geometry, black-screen state, and the
  presentation timer.
- Keep filesystem monitoring and reload state in Rust. Slint only presents
  status text that already exists in the presenter window.

## Non-goals

- A user-facing hot reload toggle or persisted preference.
- Editing or writing PDF files.
- Watching source files used to generate a PDF.
- Opening PDFium documents concurrently on multiple workers.
- Perfect change detection on every network or virtual filesystem.

## Dependency and Platform Behavior

Use the stable `notify` 8.2 release behind an application-owned watcher
abstraction. `RecommendedWatcher` selects the native backend on each supported
platform:

- macOS: FSEvents
- Windows: `ReadDirectoryChangesW`
- Linux: inotify

Watch the active PDF's parent directory non-recursively instead of watching the
file directly. Editors and document generators commonly save by renaming a
temporary file over the destination. A watch attached directly to the old file
may then follow a removed filesystem object or stop producing useful events.
The parent watch survives that replacement and reports the destination path.

Normal events are relevant only when one of their paths identifies the active
PDF. A backend rescan or missed-events indication is treated as relevant even
when it does not provide an exact child path. Event kinds and event ordering are
not interpreted as a transaction; they only indicate that the active path
should be reconciled.

The production watcher sends small signals through a bounded channel. Its
callback never accesses Slint, `AppState`, or PDFium. A UI-thread timer drains
the channel and advances the reload controller. A fake watcher and fake clock
provide deterministic unit tests.

If the native watcher cannot be created, the application falls back to
`PollWatcher`. If neither backend can watch the target, the current document
remains usable and the watcher is rebuilt with bounded exponential backoff.
Opening another PDF triggers an immediate rebuild attempt. Watcher failure must
never stop rendering or page navigation.

## Watch Lifecycle

Create the watcher when application state is initialized, but register no path
until a PDF has been committed successfully.

When a user-requested open succeeds:

1. Commit the new document and its initial rendered page.
2. Set the new path as the active document path.
3. Register the new parent directory before removing a different old parent.
4. Change the path filter to the new active PDF.

When an open fails, keep both the previous document and its watch. When two PDFs
share a parent directory, update only the path filter. Dropping the application
state stops the watcher during shutdown. A future Close PDF action can stop the
watch by setting the active target to `None`.

Paths should be made absolute without canonicalizing the final component.
Canonicalizing the destination would lose the stable pathname identity needed
to detect an atomic replacement. Symlink targets are best-effort in the first
implementation and should be documented as such.

## Reload Controller

Add a `hot_reload` module with a pure state machine similar to:

```rust
pub struct HotReloadState {
    target: Option<WatchTarget>,
    revision: u64,
    phase: HotReloadPhase,
    debounce_deadline: Option<Instant>,
    watcher_health: WatcherHealth,
}

pub enum HotReloadPhase {
    Idle,
    Debouncing,
    Preparing {
        session_id: RenderSessionId,
        revision: u64,
    },
    Failed {
        revision: u64,
    },
}
```

Suggested initial timings are:

- watcher event polling: 50 ms;
- quiet debounce: 300 ms;
- transient candidate retries: 100, 250, 500, and 1,000 ms;
- total settling window: approximately two seconds.

Every relevant signal increments `revision` and extends the quiet deadline.
Only one candidate preparation may be in flight. A signal received during
preparation marks the current revision stale; after the result arrives, the
controller prepares only the newest revision.

Temporary absence, sharing violations, an incomplete PDF, and other candidate
open failures use the short settling retries. After the settling window, retain
the last good document and wait for a later filesystem signal. Do not retry an
invalid file indefinitely.

A user-requested open has priority over hot reload. Starting one invalidates a
debouncing reload and makes any in-flight reload candidate stale. Filesystem
signals for the previous document are ignored until the manual open either
commits the new target or fails and leaves the old target active.

## Worker Transaction

Opening a candidate and immediately assigning it to `RenderWorkerState` is not
safe. The UI can reject the corresponding event as stale after the worker has
already discarded its previous document. Hot reload therefore uses a prepared
candidate and an explicit commit step.

Add commands equivalent to:

```rust
RenderCommand::PrepareReload {
    session_id: RenderSessionId,
    path: PathBuf,
    requested_page_index: u32,
}

RenderCommand::CommitReload {
    session_id: RenderSessionId,
}

RenderCommand::DiscardReload {
    session_id: RenderSessionId,
}
```

The worker stores at most one `PreparedWorkerDocument`. `PrepareReload` performs
all of the following before publishing a result:

1. Run normal PDF input preflight.
2. Open the candidate with PDFium on the existing worker.
3. Read its page count and reject an empty document.
4. Clamp `requested_page_index` to the candidate's last page.
5. Render that page at the normal current-slide width.

Only then emit a `ReloadPrepared` event containing the session ID, title, page
count, clamped page index, and rendered current page. The active document,
active render session, render queue, and cancellation token remain unchanged
while the candidate is prepared.

The UI accepts `ReloadPrepared` only when both its session and controller
revision are current. It sends `DiscardReload` for a stale candidate. For an
accepted candidate, `CommitReload` must be queued before any render, notes, or
preload work for the new session.

Control-command scheduling must preserve a queued `CommitReload` ahead of a
later user-requested `Open`; `Open` must not coalesce away or overtake that
commit. On commit, the worker:

1. clears old queued render and notes work;
2. swaps the prepared document into the active slot;
3. activates the new render session and cancels the old one;
4. accepts new-session render work.

An open or initial-page render error discards only the candidate and emits
`ReloadPrepareFailed`. The old document and session remain active.

## UI-thread Commit

Add a reload-specific session-controller transition instead of reusing the
manual-open transition. It should:

- commit only the matching pending reload session;
- increment `render_generation`;
- clear the page and thumbnail caches;
- create presentation state at the clamped current page;
- insert the prepared current page into the new cache immediately;
- replace the visible current page without showing a placeholder;
- clear speaker notes before extracting notes from the new session;
- enqueue the next-page preview, presentation preload, and thumbnails;
- leave fullscreen, window visibility, focus, geometry, black-screen state,
  and the presentation timer unchanged.

`PresentationState` should expose an `open_document_at()` constructor or an
equivalent pure transition that reuses `PageCursor` clamping.

Old cached pixels must never be inserted into the new generation. The prepared
current page is bundled with the successful candidate specifically so cache
invalidation does not create a blank frame or require old pixels to masquerade
as new content.

Speaker notes from the previous document must not remain visible after commit.
Clear them synchronously and extract the replacement notes as normal
background work.

## Presenter Status

No new controls are added to `ui/app.slint`. Reuse the existing presenter
status area without changing focus.

- Successful commit: `PDF reloaded.` for approximately two seconds.
- Candidate failure after settling: `Reload failed; showing the previous version.`
- Slow preparation: `Reloading PDF; the current version remains visible.`
- Watcher outage: `Automatic reload is temporarily unavailable.`

Transient messages need a generation token so an old timeout cannot erase a
newer warning. Repeated watcher failures should be logged rather than repeatedly
flashing the presenter status.

## Race and Failure Rules

| Situation | Required result |
| --- | --- |
| Multiple events in one save | Extend debounce and prepare once. |
| Event during preparation | Mark the candidate stale and prepare the newest revision next. |
| Candidate missing, locked, incomplete, or invalid | Keep the old document visible and usable. |
| Candidate has fewer pages | Clamp to its last page before rendering and commit. |
| Manual open starts during reload | Manual open wins; discard or ignore the reload candidate. |
| Manual open fails | Keep the old document and its watch. |
| Manual open succeeds | Commit it, then transfer the watch. |
| Fullscreen or black screen is active | Reload underneath without changing either state. |
| Old-session render event arrives after commit | Reject it through the render session boundary. |
| Watcher backend fails | Keep presenting and rebuild the watcher in the background. |

## Test Plan

Unit tests for the reload controller and fake watcher should cover:

- in-place writes and atomic rename-to-target signals;
- unrelated sibling events;
- debounce extension and burst coalescing;
- a signal arriving while preparation is in flight;
- settling retries and later-event recovery;
- target replacement only after a successful manual open;
- watcher rebuild backoff and recovery.

Render-scheduler tests with fake documents should cover:

- prepare failure leaving the active document and session unchanged;
- a prepared document remaining inactive until commit;
- stale candidate discard;
- current-page render failure preserving the active document;
- commit cancelling old work and rejecting old results;
- commit ordering relative to a later manual open.

Session-controller and presentation tests should cover:

- preserving an existing page index;
- clamping after the page count shrinks;
- clearing old pages, thumbnails, and notes;
- inserting the prepared current page into the new generation;
- preserving timer, fullscreen, and black-screen state;
- rejecting stale prepared and failure events without changing the old view.

Manual verification on macOS, Windows, and Linux should include:

- an in-place save;
- a temporary-file atomic replacement;
- rapid consecutive saves;
- temporary deletion and recreation;
- a locked, empty, truncated, and invalid replacement;
- page-count growth and shrinkage;
- reload while fullscreen and while the black screen is active.

The normal `cargo fmt --check`, `cargo check`, and `cargo test` validation
remains required for the implementation PR.

## Implementation Sequence

1. Add the pure reload controller, watcher abstraction, and unit tests.
2. Add the worker prepare/commit/discard transaction and scheduler tests.
3. Add the page-preserving UI-thread commit and cache-generation tests.
4. Connect the `notify` backend and watcher recovery lifecycle.
5. Add status integration and platform manual verification notes.

This sequence keeps the filesystem backend at the outside of the design and
establishes failure-safe document replacement before automatic events can
trigger it.
