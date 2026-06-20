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

Use File > Clear Recent Files to remove the saved list.

## Development Override

`QP_RECENT_FILES_PATH` overrides the recent-file storage path. This is intended
for tests and development workflows, not as a packaged-app privacy feature. If
it is set, Quick Presenter reads and writes the recent-file list at that exact
path.
