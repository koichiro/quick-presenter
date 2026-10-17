# Audience configuration import validation

Use this checklist to validate native import on Windows and macOS. The fixture
below contains a synthetic credential and a reserved example origin. It validates
import and persistence only; it cannot establish a working relay connection.
Record the tested commit, OS version, build mode and pass/fail results. Keep real
credentials, user paths and complete ACL output out of committed evidence.

## Windows preparation

The pull request triggers the existing Build Binaries matrix. Its Windows x64
artifact is `quick-presenter-windows-x64`; use the artifact from the exact PR run
for GUI validation. The matrix also runs the configuration import tests. Local
source-build commands below are an alternative and provide the full test suite.

Check out the pull request against `v3.0.0`, obtain PDFium using the documented
project setup, then run:

```powershell
cargo fmt --check
cargo check
cargo test
```

Create an isolated fixture instead of changing the normal user configuration:

```powershell
$fixtureDir = Join-Path $env:TEMP ("qp-import-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $fixtureDir | Out-Null
$source = Join-Path $fixtureDir "download.toml"
$destination = Join-Path $fixtureDir "config.toml"
$utf8 = [System.Text.UTF8Encoding]::new($false)
$fixture = "[audience]`ntransport = 'remote'`nrelay = 'https://relay.example'`ncreate_token = '" + ('A' * 43) + "'`n"
[System.IO.File]::WriteAllText($source, $fixture, $utf8)
[System.IO.File]::WriteAllText($destination, "[presentation]`nkeep = 'unchanged'`n", $utf8)
cargo run -- --config $destination
```

The `--config` path remains the destination for import. Keep this PowerShell
session available for the checks below. If the application restores an old PDF
that no longer exists, open a synthetic test PDF to exercise playback separately.

## Import and recovery checks

1. While stopped, choose **Import settings** and select `download.toml`. Verify
   the preview shows `https://relay.example/` without displaying its credential.
   Cancel; the destination must remain byte-for-byte unchanged.
2. Select the same file and confirm **Import**. The application must remain
   responsive after the native file picker closes. Verify the success message and
   that **Check connection and start** is available without restarting qp.
3. Inspect the isolated destination locally. The `presentation.keep` value must
   remain `unchanged`; Audience settings must match the fixture. Do not paste
   configuration contents containing real credentials into validation evidence.
4. Verify the protected ACL after replacement:

   ```powershell
   $acl = Get-Acl $destination
   $acl.AreAccessRulesProtected
   $acl.Access | Select-Object IdentityReference, FileSystemRights, IsInherited
   ```

   Expected: inheritance is disabled, entries are explicit, and full access is
   limited to OWNER RIGHTS (`S-1-3-4`) and SYSTEM (`S-1-5-18`). No broad Users or
   Everyone grant may remain. Record only a sanitized pass/fail summary.
5. Close and restart with the same `--config` path. The imported endpoint must
   load successfully with the unrelated value retained. Restart must not require
   manual file editing or another import.
6. Repeat using a malformed TOML file, missing credential, invalid origin and
   oversized file (>16 KiB). Each must show a redacted error, leave the destination
   unchanged and retain the previous endpoint. No credential may appear in errors.
7. Make the destination invalid or unwritable and attempt a valid import. Verify
   it refuses safely, leaves existing contents intact and allows correction/retry.
8. Repeat with spaces and non-ASCII characters in source and destination paths.
   Confirm the native picker, preview, persistence and restart work.

## Connection and event checks

Use a controlled relay and a disposable credential for these checks. Replace only
the isolated import fixture. The example-origin fixture above is not suitable.

- **Check connection and start** creates the normal visible Audience session and
  QR. It is an explicit start, not a separate hidden probe. Confirm audience join,
  reaction/comment delivery and PDF playback.
- Import must not replace an active room. Verify the import action is unavailable
  while running and during stop/start work; normal PDF controls remain usable.
- Stop, wait for confirmed termination, then start again and verify the new QR.
  Test interrupted/lost creation and stop responses using the existing controlled
  relay test procedures; unknown outcomes must retain their retry identity.
- Restart qp while the old room remains. Verify existing-session refusal/recovery
  guidance; a generic refusal must not be presented as proof that a room exists.

## macOS comparison

Repeat the import/cancel/invalid-file/restart checks with an isolated `--config`
path. Confirm the saved file mode is 0600 and unrelated TOML values survive. The
preview is application UI so the native picker is not followed by a nested native
confirmation dialog. A local macOS exercise verified selection, preview, confirmed
import, preserved settings and mode 0600; Windows device/ACL results remain pending.
