//! Platform process handles; Windows creation applies the token before execution.
use crate::renderer_resources::ResourceJob;
use anyhow::Result;
use std::{path::Path, process::Command};

#[cfg(not(target_os = "windows"))]
pub use std::process::{Child, ChildStdin as WritePipe, ChildStdout as ReadPipe};
#[cfg(target_os = "windows")]
pub use windows::{Child, ReadPipe, WritePipe};

#[cfg(all(test, target_os = "windows"))]
mod tests {
    #[test]
    fn child_management_can_move_between_supervision_threads() {
        fn assert_send<T: Send>() {}
        assert_send::<super::Child>();
    }
}

pub fn spawn(command: Command, document: Option<&Path>) -> Result<(Child, ResourceJob)> {
    #[cfg(target_os = "windows")]
    return windows::spawn(command, document);
    #[cfg(not(target_os = "windows"))]
    {
        let mut command = command;
        let _ = document;
        let mut child = command.spawn()?;
        match ResourceJob::attach(&child) {
            Ok(job) => Ok((child, job)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                Err(error)
            }
        }
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use anyhow::{ensure, Context};
    use std::{
        ffi::OsStr,
        fs::File,
        io,
        os::windows::{
            ffi::OsStrExt,
            io::{AsRawHandle, FromRawHandle, OwnedHandle},
            process::ExitStatusExt,
        },
        process::ExitStatus,
    };
    use windows_sys::Win32::{
        Foundation::*,
        Security::{Isolation::*, *},
        System::{Pipes::CreatePipe, Threading::*},
    };
    pub type WritePipe = File;
    pub type ReadPipe = File;
    pub struct Child {
        pub stdin: Option<File>,
        pub stdout: Option<File>,
        handle: OwnedHandle,
        pid: u32,
        _runtime: RuntimeDirectory,
        _profile: ContainerProfile,
    }
    impl AsRawHandle for Child {
        fn as_raw_handle(&self) -> std::os::windows::io::RawHandle {
            self.handle.as_raw_handle()
        }
    }
    impl Child {
        pub fn id(&self) -> u32 {
            self.pid
        }
        pub fn kill(&mut self) -> io::Result<()> {
            checked(unsafe { TerminateProcess(self.as_raw_handle(), 1) })
        }
        pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
            match unsafe { WaitForSingleObject(self.as_raw_handle(), 0) } {
                WAIT_TIMEOUT => Ok(None),
                WAIT_OBJECT_0 => self.status().map(Some),
                _ => Err(io::Error::last_os_error()),
            }
        }
        pub fn wait(&mut self) -> io::Result<ExitStatus> {
            if unsafe { WaitForSingleObject(self.as_raw_handle(), INFINITE) } != WAIT_OBJECT_0 {
                return Err(io::Error::last_os_error());
            }
            self.status()
        }
        fn status(&self) -> io::Result<ExitStatus> {
            let mut code = 0;
            checked(unsafe { GetExitCodeProcess(self.as_raw_handle(), &mut code) })?;
            Ok(ExitStatus::from_raw(code))
        }
    }
    fn checked(value: i32) -> io::Result<()> {
        if value == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
    fn wide(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain(Some(0)).collect()
    }
    unsafe fn owned(handle: HANDLE) -> OwnedHandle {
        unsafe { OwnedHandle::from_raw_handle(handle) }
    }
    fn pipe(parent_reads: bool) -> io::Result<(OwnedHandle, OwnedHandle)> {
        let attrs = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: std::ptr::null_mut(),
            bInheritHandle: 1,
        };
        let (mut read, mut write) = (std::ptr::null_mut(), std::ptr::null_mut());
        // SAFETY: valid output pointers and security attributes.
        checked(unsafe { CreatePipe(&mut read, &mut write, &attrs, 0) })?;
        let (read, write) = unsafe { (owned(read), owned(write)) };
        let parent = if parent_reads { &read } else { &write };
        checked(unsafe { SetHandleInformation(parent.as_raw_handle(), HANDLE_FLAG_INHERIT, 0) })?;
        Ok((read, write))
    }
    struct Sid(PSID);
    impl Drop for Sid {
        fn drop(&mut self) {
            unsafe {
                FreeSid(self.0);
            }
        }
    }
    struct ContainerProfile {
        name: Vec<u16>,
    }
    impl ContainerProfile {
        fn create(name: Vec<u16>) -> Result<(Self, Sid)> {
            let mut sid = std::ptr::null_mut();
            // A derived SID alone does not create the namespace/profile required
            // by CreateProcess. Never reuse another launch's writable profile.
            let status = unsafe {
                CreateAppContainerProfile(
                    name.as_ptr(),
                    name.as_ptr(),
                    name.as_ptr(),
                    std::ptr::null(),
                    0,
                    &mut sid,
                )
            };
            if status < 0 {
                return Err(io::Error::from_raw_os_error(status & 0xffff).into());
            }
            Ok((Self { name }, Sid(sid)))
        }
    }
    impl Drop for ContainerProfile {
        fn drop(&mut self) {
            // Only this launch's unique profile; no shared/user-selected state.
            unsafe {
                DeleteAppContainerProfile(self.name.as_ptr());
            }
        }
    }
    struct LocalAllocation(*mut std::ffi::c_void);
    impl Drop for LocalAllocation {
        fn drop(&mut self) {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
    struct RuntimeDirectory(std::path::PathBuf);
    impl Drop for RuntimeDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    impl RuntimeDirectory {
        fn stage(source: &Path, sid: PSID, nonce: u64) -> Result<Self> {
            use windows_sys::Win32::Security::Authorization::*;
            let path = std::env::temp_dir().join(format!(
                "quick-presenter-renderer-{}-{nonce}",
                std::process::id()
            ));
            std::fs::create_dir(&path)?;
            let runtime = Self(path);
            let mut sid_text = std::ptr::null_mut();
            checked(unsafe { ConvertSidToStringSidW(sid, &mut sid_text) })?;
            let _sid_text = LocalAllocation(sid_text.cast());
            let mut length = 0;
            while unsafe { *sid_text.add(length) } != 0 {
                length += 1;
            }
            let sid_text =
                String::from_utf16(unsafe { std::slice::from_raw_parts(sid_text, length) })?;
            // Owner/system retain cleanup authority; this unique AppContainer
            // gets read/execute only on the broker's trusted runtime copy.
            let sddl = wide(OsStr::new(&format!(
                "D:P(A;OICI;FA;;;OW)(A;OICI;FA;;;SY)(A;OICI;GRGX;;;{sid_text})"
            )));
            let mut descriptor = std::ptr::null_mut();
            checked(unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    1,
                    &mut descriptor,
                    std::ptr::null_mut(),
                )
            })?;
            let _descriptor = LocalAllocation(descriptor);
            let (mut present, mut defaulted, mut dacl) = (0, 0, std::ptr::null_mut());
            checked(unsafe {
                GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted)
            })?;
            ensure!(
                present != 0 && !dacl.is_null(),
                "missing renderer runtime DACL"
            );
            let path = wide(runtime.0.as_os_str());
            let status = unsafe {
                SetNamedSecurityInfoW(
                    path.as_ptr(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    dacl,
                    std::ptr::null_mut(),
                )
            };
            ensure!(status == 0, "renderer runtime ACL unavailable");
            std::fs::copy(source, runtime.0.join("quick-presenter.exe"))?;
            let library = crate::pdf::renderer_library_path()?;
            let directory = runtime.0.join("pdfium/bin");
            std::fs::create_dir_all(&directory)?;
            std::fs::copy(library, directory.join("pdfium.dll"))?;
            Ok(runtime)
        }
    }
    struct Attributes {
        _storage: Vec<usize>,
        pointer: LPPROC_THREAD_ATTRIBUTE_LIST,
    }
    impl Drop for Attributes {
        fn drop(&mut self) {
            unsafe {
                DeleteProcThreadAttributeList(self.pointer);
            }
        }
    }
    impl Attributes {
        fn new() -> io::Result<Self> {
            let mut bytes = 0;
            unsafe {
                InitializeProcThreadAttributeList(std::ptr::null_mut(), 2, 0, &mut bytes);
            }
            if bytes == 0 {
                return Err(io::Error::last_os_error());
            }
            let mut storage = vec![0usize; bytes.div_ceil(std::mem::size_of::<usize>())];
            let pointer = storage.as_mut_ptr().cast();
            checked(unsafe { InitializeProcThreadAttributeList(pointer, 2, 0, &mut bytes) })?;
            Ok(Self {
                _storage: storage,
                pointer,
            })
        }
        unsafe fn set<T>(&mut self, kind: u32, value: *const T, bytes: usize) -> io::Result<()> {
            unsafe {
                checked(UpdateProcThreadAttribute(
                    self.pointer,
                    0,
                    kind as usize,
                    value.cast_mut().cast(),
                    bytes,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                ))
            }
        }
    }
    pub(super) fn spawn(command: Command, document: Option<&Path>) -> Result<(Child, ResourceJob)> {
        let mut stage = "input";
        let result = (|| -> Result<(Child, ResourceJob)> {
            let mut package_length = 0;
            ensure!(
                unsafe {
                    windows_sys::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName(
                        &mut package_length,
                        std::ptr::null_mut(),
                    )
                } == APPMODEL_ERROR_NO_PACKAGE,
                "MSIX renderer isolation is not validated; refusing PDF work"
            );
            let document = document.context("AppContainer requires a broker-selected document")?;
            crate::pdf::preflight_pdf_input(document)?;
            let input = File::open(document)?;
            checked(unsafe {
                SetHandleInformation(
                    input.as_raw_handle(),
                    HANDLE_FLAG_INHERIT,
                    HANDLE_FLAG_INHERIT,
                )
            })?;
            let (child_input, parent_input) = pipe(false)?;
            let (parent_output, child_output) = pipe(true)?;
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let nonce = (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos() as u64)
                .wrapping_add(NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
            stage = "container-sid";
            let name = wide(OsStr::new(&format!(
                "QuickPresenter.Renderer.{}.{nonce}",
                std::process::id()
            )));
            let (profile, sid) = ContainerProfile::create(name)?;
            let capabilities = SECURITY_CAPABILITIES {
                AppContainerSid: sid.0,
                Capabilities: std::ptr::null_mut(),
                CapabilityCount: 0,
                Reserved: 0,
            };
            let handles = [
                child_input.as_raw_handle(),
                child_output.as_raw_handle(),
                input.as_raw_handle(),
            ];
            stage = "startup-attributes";
            let mut attributes = Attributes::new()?;
            unsafe {
                attributes.set(
                    PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES,
                    &capabilities,
                    std::mem::size_of_val(&capabilities),
                )?;
                attributes.set(
                    PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
                    handles.as_ptr(),
                    std::mem::size_of_val(&handles),
                )?;
            }
            let source = std::fs::canonicalize(command.get_program())?;
            stage = "runtime-grant";
            let runtime = RuntimeDirectory::stage(&source, sid.0, nonce)?;
            let executable = runtime.0.join("quick-presenter.exe");
            // Production passes no arbitrary arguments: only this internal helper mode.
            ensure!(
                command.get_args().eq([OsStr::new("--renderer-helper")]),
                "unexpected renderer arguments"
            );
            let mut arguments = wide(OsStr::new(&format!(
                "\"{}\" --renderer-helper --renderer-input-handle={}",
                executable.display(),
                input.as_raw_handle() as usize
            )));
            let program = wide(executable.as_os_str());
            let mut environment = Vec::<u16>::new();
            // Do not expose the broker's credentials, user paths, or unrelated env.
            let mut entries = std::collections::BTreeMap::new();
            stage = "environment";
            // Windows uses LOCALAPPDATA to establish AppContainer profile
            // redirection during creation. Its presence is not a filesystem
            // grant; the capability-free token still enforces the ACL boundary.
            for name in ["SystemRoot", "WINDIR", "LOCALAPPDATA"] {
                if let Some(value) = std::env::var_os(name) {
                    entries.insert(name.to_owned(), value);
                }
            }
            ensure!(
                entries.contains_key("SystemRoot") && entries.contains_key("LOCALAPPDATA"),
                "renderer OS environment unavailable"
            );
            for (key, value) in command.get_envs() {
                if let (Some(key), Some(value)) = (key.to_str(), value) {
                    if key.starts_with("QUICK_PRESENTER_SANDBOX_") {
                        entries.insert(key.to_owned(), value.to_owned());
                    }
                }
            }
            for (key, value) in std::env::vars_os() {
                if let Some(key) = key.to_str() {
                    let allowed = key.starts_with("QUICK_PRESENTER_SANDBOX_")
                        || (cfg!(debug_assertions)
                            && (key.starts_with("QUICK_PRESENTER_HELPER_TEST_")
                                || key == "PDFIUM_DYNAMIC_LIB_PATH"));
                    if allowed {
                        entries.insert(key.to_owned(), value);
                    }
                }
            }
            #[cfg(debug_assertions)]
            if let Some(marker) = entries.remove("QUICK_PRESENTER_HELPER_TEST_FAULT_ONCE") {
                if std::fs::remove_file(marker).is_err() {
                    entries.remove("QUICK_PRESENTER_HELPER_TEST_FAULT");
                }
            }
            // The helper must load the copied runtime, not a development path that
            // is outside its explicit read-only ACL grant.
            entries.remove("PDFIUM_DYNAMIC_LIB_PATH");
            for (key, value) in entries {
                environment.extend(wide(OsStr::new(&format!(
                    "{}={}",
                    key,
                    value.to_string_lossy()
                ))));
            }
            environment.push(0);
            let mut startup = STARTUPINFOEXW::default();
            startup.StartupInfo.cb = std::mem::size_of_val(&startup) as u32;
            startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
            startup.StartupInfo.hStdInput = child_input.as_raw_handle();
            startup.StartupInfo.hStdOutput = child_output.as_raw_handle();
            startup.StartupInfo.hStdError = child_output.as_raw_handle();
            startup.lpAttributeList = attributes.pointer;
            let mut process = PROCESS_INFORMATION::default();
            // SAFETY: buffers/attributes and inherited handles live until this call
            // completes. The OS installs AppContainer before any child instruction.
            stage = "create-appcontainer";
            checked(unsafe {
                CreateProcessW(
                    program.as_ptr(),
                    arguments.as_mut_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    1,
                    EXTENDED_STARTUPINFO_PRESENT
                        | CREATE_SUSPENDED
                        | CREATE_NO_WINDOW
                        | CREATE_UNICODE_ENVIRONMENT,
                    environment.as_ptr().cast(),
                    std::ptr::null(),
                    &startup.StartupInfo,
                    &mut process,
                )
            })?;
            let thread = unsafe { owned(process.hThread) };
            let mut child = Child {
                stdin: Some(File::from(parent_input)),
                stdout: Some(File::from(parent_output)),
                handle: unsafe { owned(process.hProcess) },
                pid: process.dwProcessId,
                _runtime: runtime,
                _profile: profile,
            };
            stage = "job-controls";
            let job = match ResourceJob::attach(&child) {
                Ok(job) => job,
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(error);
                }
            };
            stage = "resume";
            if unsafe { ResumeThread(thread.as_raw_handle()) } == u32::MAX {
                let error = io::Error::last_os_error();
                let _ = child.kill();
                let _ = child.wait();
                return Err(error.into());
            }
            Ok((child, job))
        })();
        result.map_err(|error| {
            let code = error.chain().find_map(|cause| {
                cause
                    .downcast_ref::<io::Error>()
                    .and_then(io::Error::raw_os_error)
            });
            // Only trusted setup stage and numeric OS error, never error chains
            // or PDF paths/content. This also works before logging initialization.
            eprintln!("Renderer sandbox setup failed: stage={stage} win32={code:?}");
            anyhow::anyhow!("renderer sandbox setup failed")
        })
    }

    pub(crate) fn verify_token() -> Result<()> {
        let mut token = std::ptr::null_mut();
        checked(unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) })?;
        let token = unsafe { owned(token) };
        let (mut value, mut bytes) = (0u32, 0u32);
        checked(unsafe {
            GetTokenInformation(
                token.as_raw_handle(),
                TokenIsAppContainer,
                (&mut value as *mut u32).cast(),
                4,
                &mut bytes,
            )
        })?;
        ensure!(value == 1, "renderer requires AppContainer token");
        Ok(())
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn verify_token() -> Result<()> {
    windows::verify_token()
}

#[cfg(target_os = "windows")]
pub(crate) fn take_document_file() -> Result<std::fs::File> {
    use std::os::windows::io::FromRawHandle;
    let argument = std::env::args()
        .nth(2)
        .ok_or_else(|| anyhow::anyhow!("missing brokered PDF handle"))?;
    let handle = argument
        .strip_prefix("--renderer-input-handle=")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|handle| *handle != 0 && *handle != usize::MAX)
        .ok_or_else(|| anyhow::anyhow!("invalid brokered PDF handle"))?;
    // SAFETY: this internal mode accepts the broker's explicitly inherited,
    // read-only handle once. It is never interpreted as a filesystem grant.
    Ok(unsafe { std::fs::File::from_raw_handle(handle as *mut std::ffi::c_void) })
}

#[cfg(target_os = "windows")]
pub(crate) fn verify_denials() -> Result<()> {
    let Some(sentinel) = std::env::var_os("QUICK_PRESENTER_SANDBOX_DENIAL_PROBE") else {
        return Ok(());
    };
    anyhow::ensure!(
        std::fs::File::open(&sentinel).is_err(),
        "sandbox allowed unrelated read"
    );
    anyhow::ensure!(
        std::fs::OpenOptions::new()
            .write(true)
            .open(&sentinel)
            .is_err(),
        "sandbox allowed unrelated write"
    );
    anyhow::ensure!(
        std::net::TcpListener::bind("127.0.0.1:0").is_err(),
        "sandbox allowed listen"
    );
    if let Ok(address) = std::env::var("QUICK_PRESENTER_SANDBOX_CONNECT_PROBE") {
        anyhow::ensure!(
            std::net::TcpStream::connect_timeout(
                &address.parse()?,
                std::time::Duration::from_secs(1)
            )
            .is_err(),
            "sandbox allowed connect"
        );
    }
    anyhow::ensure!(
        Command::new(std::env::current_exe()?)
            .arg("--help")
            .status()
            .is_err(),
        "sandbox allowed child creation"
    );
    Ok(())
}
