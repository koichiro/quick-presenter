# Privacy

Quick Presenter is a local PDF presentation tool. It does not upload PDF files,
speaker notes, recent files, or presentation activity.

## Recent Files

Quick Presenter stores a small recent-file list so speakers can reopen decks
from the File menu. The list contains only PDF file paths, one path per line,
and keeps at most five entries.

The recent-file list does not store PDF contents, rendered slide images,
speaker notes, thumbnails, page history, or usage analytics. File paths are
stored as plain text, so deck names and directory names may be visible to
anyone who can read the user's configuration directory.

The recent-file list is stored under the platform configuration directory:

- macOS: `~/Library/Application Support/Quick Presenter/recent-files.txt`
- Windows: `%APPDATA%\Quick Presenter\recent-files.txt`
- Linux and other Unix platforms: `$XDG_CONFIG_HOME/quick-presenter/recent-files.txt`,
  or `~/.config/quick-presenter/recent-files.txt` when `XDG_CONFIG_HOME` is not
  set

On Unix-like platforms, Quick Presenter saves `recent-files.txt` with owner-only
read/write permissions. Existing files with broader permissions are repaired the
next time the recent-file list is saved. On Windows, access control is delegated
to the user's `%APPDATA%` directory ACLs.

Use File > Clear Recent Files to remove the saved list.

## Startup Restoration

Quick Presenter saves one `startup-state.json` record containing the last active
PDF's absolute path and the audience window's last usable normal size and
placement. Placement includes display geometry and scale, but no monitor names
or persistent monitor IDs. It does not include PDF contents, slide images, notes,
page index, timer, black-screen state, fullscreen intent, or presenter placement.
Paths are readable local data (including lossless native encoding for unusual
filenames) and can reveal deck and directory names.

The record is stored beside `recent-files.txt`:

- macOS: `~/Library/Application Support/Quick Presenter/startup-state.json`
- Windows: `%APPDATA%\Quick Presenter\startup-state.json`
- Linux: `$XDG_CONFIG_HOME/quick-presenter/startup-state.json`, or
  `~/.config/quick-presenter/startup-state.json` when `XDG_CONFIG_HOME` is unset

The app reads this record only on normal GUI startup and writes it atomically
only on orderly exit when it changes. Unix files use owner-only mode `0600`;
Windows uses the configuration directory's ACLs. A failed save keeps the
previous file. Crash or forced termination may retain an older record.

Remove `startup-state.json` while the app is closed to reset placement and forget
the last PDF. File > Clear Recent Files clears only the separate recent-file list.
An explicit startup PDF takes precedence over the remembered PDF. If the
remembered PDF cannot be opened and no other PDF is successfully opened, orderly
exit clears its saved path.

`--smoke-open-pdf` and `--gui-smoke`, including report and log options, bypass
startup settings completely and never replace the remembered PDF or placement.
Wayland preserves the saved placement section while still remembering/reopening
the PDF. Quick Presenter does not upload this record.

## Diagnostic Logs

Diagnostic logs may contain local file paths, page numbers, platform lookup
decisions, and error chains. They do not contain PDF contents, rendered slide
images, speaker notes, or full file dumps, and Quick Presenter does not upload
them.

The v1.0.0 policy keeps an active log and one previous generation, with a 10 MiB
limit per generation. On Unix-like platforms, both files use owner-only mode
`0600`. See [Diagnostic Log Retention](DIAGNOSTIC_LOG_RETENTION.md) for the full
rotation, custom-path, and failure-handling policy.

## Development Override

`QP_RECENT_FILES_PATH` overrides the recent-file storage path. This is intended
for tests and development workflows, not as a packaged-app privacy feature. If
it is set, Quick Presenter reads and writes the recent-file list at that exact
path. On Unix-like platforms, the same owner-only file permissions are applied
when saving through this override, but parent directory permissions depend on the
chosen path.
