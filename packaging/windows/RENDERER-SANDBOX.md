# Windows renderer isolation (#375, draft validation gate)

The proposed supported matrix is unpackaged Windows 10/11 x64, including the
direct MSI. MSIX/Store identity is explicitly rejected before helper startup:
this branch does not claim a tested nested-container/Store configuration.
No Win32 App Isolation preview or administrative token privilege is required.

The sanitized launch environment preserves only required OS bootstrap paths
(`SystemRoot`, `WINDIR`, `LOCALAPPDATA`) and explicit sandbox/test probes.
`LOCALAPPDATA` is required for Windows AppContainer profile redirection; retaining
its value grants no access to unrelated files. Credentials and unrelated parent
environment variables are not inherited.

The broker uses `STARTUPINFOEX` security capabilities to create a distinct
capability-free AppContainer token **before any child instruction executes**.
Each helper receives a unique container profile and SID (not a shared identity).
The profile is created before launch and removed on normal teardown. Windows
grants that container its own private profile storage/registry; this is not a
claim of zero writable storage. No profile is reused between helper launches.
Only explicitly listed stdin/stdout and the selected read-only PDF handle are
inherited. The PDF path in IPC supplies the display name; PDFium reads the owned
reader and never opens that user path. No ACL is changed on the original PDF,
home directory, or installed executable.

The broker copies its trusted executable and selected bundled PDFium DLL into
a unique temporary runtime directory. A protected DACL retains owner/system
cleanup access and grants this helper SID read/execute only, inherited by copied
runtime files. It does not grant write, device, credential, network, registry,
COM, or user-interface capabilities. Classic AppContainer can still access
Windows resources explicitly granted to ALL APPLICATION PACKAGES; this is not
an LPAC claim or a claim that Windows exposes no public system resources.
The runtime copy preserves executable/DLL signatures. Normal teardown deletes
it; abrupt broker termination can leave trusted runtime copies and private
container profiles, but the broker never copies PDF content into either.
Never treat these leftover copies as authoritative installed artifacts.

The child is initially suspended. Before resuming it, the broker assigns a
non-inherited Job Object with:

- 1 GiB committed-memory limit;
- one active process (child creation cannot escape the job);
- kill-on-job-close and native-error-dialog suppression;
- 80% aggregate CPU hard cap, leaving presenter headroom while operation wall
  deadlines still bound individual work;
- all eight documented basic UI restrictions (handles, clipboard, atoms,
  desktop/display/system settings, and exit-Windows).

Any token, ACL, process, job, or resume failure fails closed. No unsandboxed
launcher fallback exists. The helper checks `TokenIsAppContainer` before parsing
protocol traffic. Raw protocol harnesses alone opt into an explicit debug-only
test path; release binaries ignore it, and normal broker process tests exercise
the actual launcher.

## Validation still required before making a release claim

The Rust launcher/resource-control modules are Windows-target type-checked and
shared behavior is regression-tested on macOS. **No local Windows runtime test
has been performed. Keep the PR draft until native Windows CI and the installed
MSI gate establish token launch, ACL inheritance, PDF open/render/notes, denied
private read/write, denied listen/connect/child creation, and termination after
broker crash.** AppContainer handle inheritance and the protected runtime DACL
must be tested, not inferred from successful compilation.

CI installs the actual per-machine MSI on a disposable Windows runner, runs
the denial gate from outside the install directory with PDFium overrides unset,
and uninstalls it in a finally block. Administrative MSI extraction remains a
separate layout check, not evidence of installed-package confinement.

Run the release-capable staged/installed gate:

```powershell
python scripts/check_renderer_sandbox.py "C:\Program Files\Quick Presenter\quick-presenter.exe" tests/fixtures/marp-speaker-notes.pdf
```

References: [AppContainer launch](https://learn.microsoft.com/en-us/windows/win32/secauthz/implementing-an-appcontainer),
[AppContainer isolation](https://learn.microsoft.com/en-us/windows/win32/secauthz/appcontainer-isolation),
[Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects).
