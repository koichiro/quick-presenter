# Diagnostic Log Retention

## Status

This document defines the v1.0.0 design for bounding diagnostic log storage.
Implementation is tracked by issue #340.

## Goals

- Keep enough recent diagnostics to investigate startup and presentation
  failures.
- Prevent packaged applications from growing the diagnostic log indefinitely.
- Protect file paths and other local diagnostic context from other Unix users.
- Keep logging failures non-fatal for normal packaged-app startup.
- Avoid touching files that Quick Presenter does not own.

## Retention policy

Quick Presenter will keep two files per configured log path:

- the active log at the configured path, such as `quick-presenter.log`;
- one previous generation at `<configured-path>.1`, such as
  `quick-presenter.log.1`.

Each generation has a 10 MiB byte limit, for a maximum retained payload of
approximately 20 MiB. The small amount of filesystem metadata is outside this
budget. No date-based retention or additional generations are used for
v1.0.0.

Rotation occurs on the diagnostics writer thread before a write would make the
active generation exceed 10 MiB. The writer removes only the exact `.1` path,
moves the active file to `.1`, and creates a new active file. A single log
record larger than the generation limit is truncated to its final 10 MiB before
it is written; this defensive case must not allow a record to bypass the
budget.

At startup, an existing active or previous generation larger than 10 MiB is
trimmed to its newest 10 MiB before normal logging starts. This migration keeps
the most recent diagnostic context while bringing logs created by older
versions under the new budget.

The implementation must never enumerate the parent directory or delete files
based on a wildcard. It may access only the configured active path and the
single derived `.1` path.

## Default and custom paths

The same size limit, generation count, rotation naming, and Unix file mode apply
to both the platform-default log and a path supplied through `--log-file`.

For a custom path, Quick Presenter considers `<custom-path>.1` part of that log
destination. The application does not modify permissions on the parent
directory. Users and packaged smoke scripts should therefore choose a dedicated
path whose active and `.1` files can both be managed by Quick Presenter.

## Permissions

On Unix-like platforms, the active and previous generations use mode `0600`.
New files are created with owner-only access, and broader permissions on an
existing active or `.1` file are repaired before it is used. Directory
permissions remain governed by the platform directory or the user-selected
custom path.

On Windows, file access continues to inherit ACLs from the per-user
`%LOCALAPPDATA%` directory or from the directory containing a custom path.
Quick Presenter does not replace inherited Windows ACLs in v1.0.0.

## Failure behavior

Default logging is best-effort and must not prevent the presentation app from
starting. If permission repair, migration, rotation, cleanup, or reopening the
default log fails, Quick Presenter reports the failure to standard error when
available, disables file logging for that process, and continues with standard
error logging. Existing log files are left in the most recoverable state
available; the application does not broaden their permissions to recover.

An explicit `--log-file` path remains a requested startup contract. Failure to
prepare, rotate, secure, or open that destination returns a startup error, as it
does today, so release smoke scripts do not silently lose their requested
diagnostic artifact.

Rotation must be ordered to preserve the active log whenever possible:

1. Flush the active file.
2. Remove only the exact previous-generation path when it exists.
3. Move the active file to the previous-generation path.
4. Create and secure the new active file.

If the final creation step fails after the move, the previous generation remains
available and the process follows the default or explicit-path behavior above.

## Implementation boundary

Rotation belongs in `src/diagnostics.rs`, behind the existing diagnostics
initialization boundary. A single writer owned by `tracing_appender` will own
the active file and perform byte accounting and rotation. Callers and Slint UI
code will not manage log files or retention state.

The implementation should isolate filesystem operations behind a small internal
boundary only where needed to inject failures in tests. It should not introduce
a general logging framework or background cleanup service.

## Verification

Focused tests must cover:

- writes immediately below, at, and above the 10 MiB boundary;
- rotation from the active file to exactly one `.1` generation;
- repeated rotations replacing `.1` without creating more generations;
- startup migration of oversized active and previous files;
- a single oversized log record;
- preservation of unrelated sibling files;
- fallback behavior for default-path rotation and permission failures;
- startup errors for equivalent failures on an explicit `--log-file` path;
- creation and repair of mode `0600` on Unix-like platforms.

The normal Rust validation remains:

```sh
cargo fmt --check
cargo check
cargo test
```

