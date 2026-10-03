//! Unprivileged Linux authority reduction, installed while still single-threaded.
use anyhow::{ensure, Result};
use std::{
    fs::File,
    os::fd::{AsRawFd, FromRawFd},
    path::Path,
};

const READ_FILE: u64 = 1 << 2;
const READ_DIR: u64 = 1 << 3;
const MIN_ABI: i64 = 3;
#[repr(C)]
struct Ruleset {
    handled_access_fs: u64,
}
#[repr(C, packed)]
struct PathRule {
    allowed_access: u64,
    parent_fd: i32,
}

pub fn close_inherited_descriptors() -> Result<()> {
    let parent = std::env::var("QUICK_PRESENTER_RENDERER_PARENT_PID")
        .ok()
        .and_then(|pid| pid.parse::<libc::pid_t>().ok())
        .unwrap_or_else(|| unsafe { libc::getppid() });
    ensure!(
        unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL, 0, 0, 0) } == 0
            && unsafe { libc::getppid() } == parent,
        "renderer parent lifetime control unavailable"
    );
    // Preserve only the three broker-selected standard streams. This is called
    // before PDFium initialization or any thread creation.
    ensure!(
        unsafe { libc::syscall(libc::SYS_close_range, 3u32, u32::MAX, 0u32) } == 0,
        "renderer descriptor confinement unavailable"
    );
    Ok(())
}

pub fn enter(document: &Path) -> Result<()> {
    ensure!(
        std::env::var_os("FLATPAK_ID").is_none() && !Path::new("/run/.flatpak-info").exists(),
        "Flatpak renderer policy is not validated; refusing PDF work"
    );
    #[cfg(debug_assertions)]
    ensure!(
        std::env::var_os("QUICK_PRESENTER_HELPER_TEST_NO_LANDLOCK").is_none(),
        "renderer requires Landlock ABI 3 or newer; refusing PDF work"
    );
    crate::pdf::initialize_renderer_runtime()?;
    ensure!(
        std::fs::read_dir("/proc/self/task")?.count() == 1,
        "renderer confinement requires a single thread"
    );
    let abi = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            std::ptr::null::<Ruleset>(),
            0usize,
            1u32,
        )
    };
    ensure!(
        abi >= MIN_ABI,
        "renderer requires Landlock ABI 3 or newer; refusing PDF work"
    );
    // ABI 2 added REFER, ABI 3 TRUNCATE, ABI 5 device IOCTL. Handle every
    // available filesystem right in this reviewed ABI range; grant only reads.
    let rights = if abi >= 5 {
        (1u64 << 16) - 1
    } else {
        (1u64 << 15) - 1
    };
    let attribute = Ruleset {
        handled_access_fs: rights,
    };
    let fd = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            &attribute,
            std::mem::size_of_val(&attribute),
            0u32,
        )
    };
    ensure!(fd >= 0, "renderer Landlock ruleset unavailable");
    // SAFETY: the syscall returned a new owned descriptor.
    let ruleset = unsafe { File::from_raw_fd(fd as i32) };
    add_read_rule(&ruleset, document)?;
    for path in [
        "/etc/fonts",
        "/usr/share/fonts",
        "/usr/share/fontconfig",
        "/var/cache/fontconfig",
    ] {
        let path = Path::new(path);
        if path.exists() {
            add_read_rule(&ruleset, path)?;
        }
    }
    #[cfg(debug_assertions)]
    if let Some(marker) = std::env::var_os("QUICK_PRESENTER_HELPER_TEST_FAULT_ONCE") {
        if Path::new(&marker).exists() {
            // Consume the test-owned marker before confinement; report it via
            // an in-process debug flag rather than granting filesystem writes.
            crate::renderer_helper::consume_fault_marker(Path::new(&marker))?;
        }
    }
    ensure!(
        unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } == 0,
        "renderer no_new_privs unavailable"
    );
    ensure!(
        unsafe { libc::syscall(libc::SYS_landlock_restrict_self, ruleset.as_raw_fd(), 0u32) } == 0,
        "renderer Landlock enforcement unavailable"
    );
    drop(ruleset);
    install_seccomp()?;
    verify_denials()?;
    Ok(())
}

fn add_read_rule(ruleset: &File, path: &Path) -> Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let path_text = std::ffi::CString::new(path.as_os_str().as_bytes())?;
    // O_PATH obtains an inode grant, not an ambient pathname prefix grant.
    let fd = unsafe { libc::open(path_text.as_ptr(), libc::O_PATH | libc::O_CLOEXEC) };
    ensure!(fd >= 0, "renderer resource grant unavailable");
    let file = unsafe { File::from_raw_fd(fd) };
    let rule = PathRule {
        allowed_access: READ_FILE
            | if file.metadata()?.is_dir() {
                READ_DIR
            } else {
                0
            },
        parent_fd: fd,
    };
    ensure!(
        unsafe {
            libc::syscall(
                libc::SYS_landlock_add_rule,
                ruleset.as_raw_fd(),
                1u32,
                &rule,
                0u32,
            )
        } == 0,
        "renderer resource rule unavailable"
    );
    Ok(())
}

const ALLOW: u32 = 0x7fff0000;
const KILL: u32 = 0x80000000;
const ERRNO: u32 = 0x00050000;
fn statement(code: u16, k: u32) -> libc::sock_filter {
    libc::sock_filter {
        code,
        jt: 0,
        jf: 0,
        k,
    }
}
fn equal(k: u32, yes: u8, no: u8) -> libc::sock_filter {
    libc::sock_filter {
        code: 0x15,
        jt: yes,
        jf: no,
        k,
    }
}

fn filter(architecture: u32, pid: u32) -> Vec<libc::sock_filter> {
    let mut code = vec![
        statement(0x20, 4),
        equal(architecture, 1, 0),
        statement(0x06, KILL),
        statement(0x20, 0),
        equal(libc::SYS_clone3 as u32, 0, 1),
        statement(0x06, ERRNO | libc::ENOSYS as u32),
        // Only same-process threads; glibc falls back here after clone3 ENOSYS.
        equal(libc::SYS_clone as u32, 0, 5),
        statement(0x20, 16),
        statement(0x54, libc::CLONE_THREAD as u32),
        equal(libc::CLONE_THREAD as u32, 0, 1),
        statement(0x06, ALLOW),
        statement(0x06, ERRNO | libc::EPERM as u32),
        equal(libc::SYS_tgkill as u32, 0, 4),
        statement(0x20, 16),
        equal(pid, 0, 1),
        statement(0x06, ALLOW),
        statement(0x06, ERRNO | libc::EPERM as u32),
        equal(libc::SYS_prlimit64 as u32, 0, 5),
        statement(0x20, 16),
        equal(0, 1, 0),
        equal(pid, 0, 1),
        statement(0x06, ALLOW),
        statement(0x06, ERRNO | libc::EPERM as u32),
        statement(0x20, 0),
    ];
    // Measured ARM64 Rust/PDFium open/render/notes surface, plus libc signal,
    // thread teardown, filesystem/font enumeration and timed wait counterparts.
    // Syscall constants are compiled separately for each claimed architecture.
    for syscall in [
        libc::SYS_read,
        libc::SYS_write,
        libc::SYS_close,
        libc::SYS_lseek,
        libc::SYS_pread64,
        libc::SYS_openat,
        libc::SYS_newfstatat,
        libc::SYS_fstat,
        libc::SYS_statx,
        libc::SYS_faccessat,
        libc::SYS_faccessat2,
        libc::SYS_getdents64,
        libc::SYS_getcwd,
        libc::SYS_fcntl,
        libc::SYS_mmap,
        libc::SYS_munmap,
        libc::SYS_mprotect,
        libc::SYS_madvise,
        libc::SYS_brk,
        libc::SYS_futex,
        libc::SYS_restart_syscall,
        libc::SYS_ppoll,
        libc::SYS_rt_sigaction,
        libc::SYS_rt_sigprocmask,
        libc::SYS_rt_sigreturn,
        libc::SYS_sigaltstack,
        libc::SYS_gettid,
        libc::SYS_getpid,
        libc::SYS_set_robust_list,
        libc::SYS_set_tid_address,
        libc::SYS_rseq,
        libc::SYS_sched_getaffinity,
        libc::SYS_sched_yield,
        libc::SYS_getrandom,
        libc::SYS_clock_gettime,
        libc::SYS_clock_nanosleep,
        libc::SYS_exit,
        libc::SYS_exit_group,
    ] {
        code.push(equal(syscall as u32, 0, 1));
        code.push(statement(0x06, ALLOW));
    }
    code.push(statement(0x06, ERRNO | libc::EPERM as u32));
    code
}

fn install_seccomp() -> Result<()> {
    #[cfg(target_arch = "aarch64")]
    let architecture = 0xc00000b7;
    #[cfg(target_arch = "x86_64")]
    let architecture = 0xc000003e;
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    anyhow::bail!("renderer seccomp architecture is unsupported");
    #[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
    {
        let mut instructions = filter(architecture, std::process::id());
        let program = libc::sock_fprog {
            len: instructions.len().try_into()?,
            filter: instructions.as_mut_ptr(),
        };
        // SAFETY: valid bounded BPF program, irreversible no_new_privs filter.
        ensure!(
            unsafe { libc::prctl(libc::PR_SET_SECCOMP, 2, &program) } == 0,
            "renderer seccomp policy unavailable"
        );
        Ok(())
    }
}

fn verify_denials() -> Result<()> {
    let Some(sentinel) = std::env::var_os("QUICK_PRESENTER_SANDBOX_DENIAL_PROBE") else {
        return Ok(());
    };
    ensure!(
        std::fs::File::open(&sentinel).is_err(),
        "sandbox allowed unrelated read"
    );
    ensure!(
        std::fs::OpenOptions::new()
            .write(true)
            .open(&sentinel)
            .is_err(),
        "sandbox allowed unrelated write"
    );
    ensure!(
        std::net::TcpListener::bind("127.0.0.1:0").is_err(),
        "sandbox allowed listen"
    );
    if let Ok(address) = std::env::var("QUICK_PRESENTER_SANDBOX_CONNECT_PROBE") {
        ensure!(
            std::net::TcpStream::connect_timeout(
                &address.parse()?,
                std::time::Duration::from_secs(1)
            )
            .is_err(),
            "sandbox allowed connect"
        );
    }
    ensure!(
        std::process::Command::new("/usr/bin/true")
            .status()
            .is_err(),
        "sandbox allowed child execution"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn evaluate(syscall: i64, arch: u32, argument: u32) -> u32 {
        let code = filter(0xc00000b7, 42);
        let (mut pc, mut value) = (0, 0);
        loop {
            let instruction = code[pc];
            match instruction.code {
                0x20 => {
                    value = match instruction.k {
                        0 => syscall as u32,
                        4 => arch,
                        16 => argument,
                        _ => panic!("unexpected load"),
                    }
                }
                0x54 => value &= instruction.k,
                0x15 => {
                    pc += if value == instruction.k {
                        instruction.jt
                    } else {
                        instruction.jf
                    } as usize
                }
                0x06 => return instruction.k,
                _ => panic!("unexpected opcode"),
            }
            pc += 1;
        }
    }
    #[test]
    fn bpf_enforces_architecture_threads_and_self_only_operations() {
        assert_eq!(evaluate(libc::SYS_read, 0xc00000b7, 0), ALLOW);
        assert_eq!(evaluate(libc::SYS_read, 0xc000003e, 0), KILL);
        for syscall in [
            libc::SYS_socket,
            libc::SYS_ptrace,
            libc::SYS_execve,
            libc::SYS_unshare,
        ] {
            assert_eq!(evaluate(syscall, 0xc00000b7, 0), ERRNO | libc::EPERM as u32);
        }
        assert_eq!(
            evaluate(libc::SYS_clone, 0xc00000b7, libc::CLONE_THREAD as u32),
            ALLOW
        );
        assert_eq!(
            evaluate(libc::SYS_clone, 0xc00000b7, libc::CLONE_VFORK as u32),
            ERRNO | libc::EPERM as u32
        );
        assert_eq!(
            evaluate(libc::SYS_clone3, 0xc00000b7, 0),
            ERRNO | libc::ENOSYS as u32
        );
        assert_eq!(evaluate(libc::SYS_tgkill, 0xc00000b7, 42), ALLOW);
        assert_eq!(
            evaluate(libc::SYS_tgkill, 0xc00000b7, 43),
            ERRNO | libc::EPERM as u32
        );
        assert_eq!(evaluate(libc::SYS_prlimit64, 0xc00000b7, 0), ALLOW);
        assert_eq!(
            evaluate(libc::SYS_prlimit64, 0xc00000b7, 43),
            ERRNO | libc::EPERM as u32
        );
    }
}
