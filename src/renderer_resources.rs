//! OS resource controls. This is not a filesystem/network security sandbox.
use crate::renderer_limits::MAX_HELPER_MEMORY_BYTES;
use anyhow::{Context, Result};
use std::{process::Child, sync::Mutex};

pub fn constrain_helper() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // App Sandbox alone is not a blanket fork prohibition. Native parsing
        // needs threads, not additional processes; keep the hard process limit 0.
        if unsafe { libc::setrlimit(libc::RLIMIT_NPROC, &limit) } != 0 {
            return Err(std::io::Error::last_os_error())
                .context("helper process limit unavailable");
        }
    }
    #[cfg(target_os = "linux")]
    {
        let limit = libc::rlimit {
            rlim_cur: MAX_HELPER_MEMORY_BYTES as libc::rlim_t,
            rlim_max: MAX_HELPER_MEMORY_BYTES as libc::rlim_t,
        };
        // SAFETY: valid rlimit pointer, only this helper's limits are modified.
        if unsafe { libc::setrlimit(libc::RLIMIT_AS, &limit) } != 0 {
            return Err(std::io::Error::last_os_error())
                .context("helper address-space limit unavailable");
        }
    }
    #[cfg(unix)]
    {
        let limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: valid rlimit pointer. Prevent PDF/notes appearing in native core dumps.
        if unsafe { libc::setrlimit(libc::RLIMIT_CORE, &limit) } != 0 {
            return Err(std::io::Error::last_os_error())
                .context("helper core-dump limit unavailable");
        }
    }
    Ok(())
}

pub fn memory_exceeded(child: &Mutex<Child>) -> bool {
    #[cfg(target_os = "macos")]
    {
        let id = child.lock().unwrap_or_else(|e| e.into_inner()).id();
        let mut usage = std::mem::MaybeUninit::<libc::rusage_info_v2>::zeroed();
        // SAFETY: flavor matches the initialized output structure and its size.
        let result = unsafe {
            libc::proc_pid_rusage(
                id as libc::c_int,
                libc::RUSAGE_INFO_V2,
                usage.as_mut_ptr().cast(),
            )
        };
        if result == 0 {
            // SAFETY: successful proc_pid_rusage initialized the structure.
            return unsafe { usage.assume_init() }.ri_resident_size
                > MAX_HELPER_MEMORY_BYTES as u64;
        }
        // Process exit races are handled by the client. Sampling is best effort,
        // not a reservation/pre-allocation cap on macOS.
    }
    let _ = child;
    false
}

#[cfg(target_os = "windows")]
pub struct ResourceJob {
    _handle: std::os::windows::io::OwnedHandle,
}
#[cfg(not(target_os = "windows"))]
pub struct ResourceJob;
impl ResourceJob {
    pub fn attach(child: &Child) -> Result<Self> {
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
            use windows_sys::Win32::System::JobObjects::*;
            // SAFETY: null security/name create an anonymous non-inherited job.
            let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if handle.is_null() {
                return Err(std::io::Error::last_os_error())
                    .context("helper memory job unavailable");
            }
            // SAFETY: take ownership of the newly created job handle exactly once.
            let handle = unsafe { OwnedHandle::from_raw_handle(handle) };
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_PROCESS_MEMORY
                | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
                | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION;
            limits.ProcessMemoryLimit = MAX_HELPER_MEMORY_BYTES;
            // SAFETY: structure size/type match the information class; valid handles.
            let set = unsafe {
                SetInformationJobObject(
                    handle.as_raw_handle(),
                    JobObjectExtendedLimitInformation,
                    (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    std::mem::size_of_val(&limits) as u32,
                )
            };
            let assigned = set != 0
                && unsafe {
                    AssignProcessToJobObject(handle.as_raw_handle(), child.as_raw_handle())
                } != 0;
            anyhow::ensure!(assigned, "helper memory job could not be assigned");
            return Ok(Self { _handle: handle });
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = child;
            Ok(Self)
        }
    }
}
