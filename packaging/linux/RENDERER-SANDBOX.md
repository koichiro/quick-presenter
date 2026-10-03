# Direct-package Linux renderer confinement (#376)

The direct Debian/standalone renderer requires Landlock ABI 3 (normally Linux
6.2+) enabled by the distribution, `close_range`, `no_new_privs`, and seccomp-BPF.
Older/disabled kernels **fail closed before PDF parsing**. There is no silent
unsandboxed fallback, root helper, setuid installation, or namespace dependency.
Debian 12's default 6.1 kernel is below this boundary; use a compatible kernel.
Unsupported architectures fail closed; the policy targets x86_64 and aarch64.

Before any helper-created thread exists, the helper closes inherited descriptors
above stderr, initializes the trusted PDFium runtime without loading a PDF, and
verifies `/proc/self/task` contains exactly one thread. SIGKILL parent-death
notification is installed with a broker-PID race check; seccomp later denies
changing it. Pipe EOF remains an independent lifetime mechanism. It canonicalizes the
selected PDF and installs a read-only inode grant. Font resources are granted
read-only at `/etc/fonts`, `/usr/share/fonts`, `/usr/share/fontconfig` and
`/var/cache/fontconfig`, when present. User-font directories, home directories,
devices, write/create/remove/execute rights and broad `/usr` grants are omitted.
ABIs 3/4 handle all filesystem rights through TRUNCATE; ABI 5+ also handles
device IOCTL. Future ABIs receive the same explicitly reviewed rights mask;
unknown/new syscalls remain denied by seccomp. Landlock does not restrict every
metadata operation (e.g. stat); this policy denies unrelated **file contents**,
not the existence of arbitrary pathnames.

Landlock is installed before starting the pipe guardian, avoiding an unrestricted
sibling thread on pre-TSYNC ABIs. Both the guardian and later same-process
threads inherit confinement. `no_new_privs` cannot be removed. Seccomp checks the
native audit architecture, denies x32/foreign ABIs, returns ENOSYS for clone3
so glibc uses the reviewed clone path, allows clone only with CLONE_THREAD,
and confines tgkill/prlimit64 to the current process. Socket, ptrace, execution,
namespace, credential, mount, device ioctl, and unlisted syscall operations
return EPERM. Existing broker termination, deadlines and EOF lifetime handling
remain independent of native PDF work.

## Seccomp evidence and gates

The allowlist was derived from native aarch64 `strace -f` of the actual Rust
helper/PDFium 8076 open/render/notes path, with explicit libc signal/thread/time
and font-enumeration counterparts. Source comments identify these additions.
The same syscall semantics are compiled against each architecture's libc
constants; **x86_64 native/package verification is a required CI gate, not a
claim derived from ARM-only testing**. Pure BPF tests verify audit architecture,
thread-only clone, forbidden socket/ptrace/exec/namespace calls, and self-only
signals/resource changes. Packaged denial tests must be rerun after PDFium,
Rust/libc, architecture, or distribution changes; never add an unexplained broad
allow merely to make a test pass.

`fcntl` is limited to F_DUPFD_CLOEXEC, F_GETFD, F_SETFD, F_GETFL and F_SETFL;
F_SETFL rejects O_ASYNC. Ownership, signal selection, leases and unknown
commands are denied. This prevents inherited pipe notifications from signaling
the broker without kill/tgkill. Native regression tests attempt the former
F_SETOWN/F_SETSIG/O_ASYNC escape over stdin and require the broker to survive
and finish PDF rendering. Staged/installed gates also assert EPERM for these
signal-configuration operations.

```sh
python3 scripts/check_renderer_sandbox.py /path/to/installed/quick-presenter tests/fixtures/marp-speaker-notes.pdf
```

The gate verifies private file read/write, loopback listen/connect and process
execution rejection while opening/rendering/extracting notes. Process regression
tests include crash/hang/malformed recovery and shutdown; debug-only injections
also demonstrate unavailable Landlock and Flatpak modes cannot downgrade.
Release builds ignore failure injections.

## Flatpak and namespaces

Flatpak is not implemented or advertised as supported by this branch. An
identified Flatpak invocation is rejected. A future manifest needs document
portal access and no broad host filesystem/network/device grants; its outer
sandbox cannot substitute for a separately reviewed native renderer boundary.
Unprivileged PID/network/mount namespaces are not required because distribution
policy and container environments vary. Landlock plus a syscall allowlist
provides the current authority boundary without relying on namespace creation.

References: [Landlock compatibility and inheritance](https://docs.kernel.org/userspace-api/landlock.html),
[seccomp userspace filtering](https://docs.kernel.org/userspace-api/seccomp_filter.html).
