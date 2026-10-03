# Windows renderer isolation (#375, draft validation gate)

The target matrix is Windows 10/11 x64, including direct MSI and installed
MSIX/Store packages. A packaged full-trust broker uses the same capability-free
AppContainer helper as an unpackaged broker; package identity must not disable
PDF rendering. There is no unsandboxed fallback for either distribution path.
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

Network isolation is a communication boundary, not a ban on creating every
socket: Windows may permit localhost bind/listen and enforce AppContainer
isolation at receive/accept. The gate actively attempts connections from an
external process while the renderer owns a listener, and requires both that
external connects fail and the renderer accepts no connection. Outbound access
to a broker-owned listener is separately denied. An unsandboxed loopback control
must succeed first. No loopback exemption or network capability is installed.
The guarantee depends on Windows network isolation/WFP configuration; it is not
a custom syscall filter. See [AppContainer network enforcement analysis](https://projectzero.google/2021/08/understanding-network-access-windows-app.html).

The runtime copy preserves executable/DLL signatures. Normal teardown deletes
it; abrupt broker termination can leave trusted runtime copies and private
container profiles, but the broker never copies PDF content into either.
Never treat these leftover copies as authoritative installed artifacts.

Before creating the child, the broker configures a non-inherited Job Object.
`PROC_THREAD_ATTRIBUTE_JOB_LIST` assigns the initially suspended child to that
job as part of process creation, so even a broker crash immediately after
`CreateProcessW` cannot leave an unassigned suspended helper. The job has:

- 1 GiB committed-memory limit;
- one active process (child creation cannot escape the job);
- kill-on-job-close and native-error-dialog suppression;
- 80% aggregate CPU hard cap, leaving presenter headroom while operation wall
  deadlines still bound individual work;
- all eight documented basic UI restrictions (handles, clipboard, atoms,
  desktop/display/system settings, and exit-Windows).

Native process tests abort the broker both immediately after process creation
(before resume) and after opening a PDF, and require the helper to terminate.

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
private read/write, denied inbound/outbound communication and child creation, and termination after
broker crash.** AppContainer handle inheritance and the protected runtime DACL
must be tested, not inferred from successful compilation.

CI installs the actual per-machine MSI on a disposable Windows runner, runs
the denial gate from outside the install directory with PDFium overrides unset,
and uninstalls it in a finally block. Administrative MSI extraction remains a
separate layout check, not evidence of installed-package confinement.
WiX must build with `-arch x64`; CI verifies the MSI Template Summary before
installation. Without this, a default x86 MSI redirects Program Files to (x86),
even when its payload executable is x64 or INSTALLFOLDER is supplied explicitly.

CI also builds an MSIX with the Store identity, signs a temporary copy with a
disposable test certificate, installs it, and runs the render/notes and denial
probe in its package context using `Invoke-CommandInDesktopPackage`. The broker
asserts package identity, so unpacked execution cannot satisfy this gate. The
package and certificate are removed afterwards; this test package is not a
release artifact. This validates installed package-context behavior, not Store
certification or the final interactive Store activation/GUI release checklist.

Windows helper stderr is not the framed protocol stream. Denial-probe failures
are bounded IPC errors before native PDF work, so setup diagnostics cannot
corrupt the handshake or expose raw PDF/native errors through stdout.

Run the release-capable staged/installed gate:

```powershell
python scripts/check_renderer_sandbox.py "C:\Program Files\Quick Presenter\quick-presenter.exe" tests/fixtures/marp-speaker-notes.pdf
```

References: [AppContainer launch](https://learn.microsoft.com/en-us/windows/win32/secauthz/implementing-an-appcontainer),
[AppContainer isolation](https://learn.microsoft.com/en-us/windows/win32/secauthz/appcontainer-isolation),
[Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects).
