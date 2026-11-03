//! Verify the permissions of files produced by the shared settings writer.
#![cfg(target_os = "windows")]

#[allow(dead_code)]
#[path = "../src/settings_storage.rs"]
mod settings_storage;

use std::{ffi::c_void, fs, os::windows::ffi::OsStrExt, path::Path, ptr};
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            GetNamedSecurityInfoW, SDDL_REVISION_1, SE_FILE_OBJECT,
        },
        GetAce, GetSecurityDescriptorControl, SetFileSecurityW, ACCESS_ALLOWED_ACE,
        DACL_SECURITY_INFORMATION, INHERITED_ACE, PROTECTED_DACL_SECURITY_INFORMATION,
        SE_DACL_PROTECTED,
    },
    Storage::FileSystem::FILE_ALL_ACCESS,
};

struct LocalAllocation(*mut c_void);

impl Drop for LocalAllocation {
    fn drop(&mut self) {
        // SAFETY: Each instance owns an allocation returned by a Windows security API.
        unsafe { LocalFree(self.0) };
    }
}

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain([0]).collect()
}

fn allow_inherited_public_access(path: &Path) {
    let sddl: Vec<u16> = "D:P(A;OICI;FA;;;WD)".encode_utf16().chain([0]).collect();
    let mut descriptor = ptr::null_mut();
    // SAFETY: All input strings are terminated, output pointers are valid, and the
    // descriptor remains allocated until SetFileSecurityW has returned.
    unsafe {
        assert_ne!(
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                ptr::null_mut(),
            ),
            0
        );
        let _allocation = LocalAllocation(descriptor);
        assert_ne!(
            SetFileSecurityW(
                wide(path).as_ptr(),
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                descriptor,
            ),
            0
        );
    }
}

fn assert_private_acl(path: &Path) {
    let mut descriptor = ptr::null_mut();
    let mut dacl = ptr::null_mut();
    // SAFETY: Windows supplies the descriptor and its ACL/ACE/SID pointers.
    // The descriptor stays allocated throughout inspection, and every successful
    // SID conversion is freed independently after copying the terminated string.
    unsafe {
        assert_eq!(
            GetNamedSecurityInfoW(
                wide(path).as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                &mut dacl,
                ptr::null_mut(),
                &mut descriptor,
            ),
            0
        );
        let _descriptor = LocalAllocation(descriptor);
        let mut control = 0;
        let mut revision = 0;
        assert_ne!(
            GetSecurityDescriptorControl(descriptor, &mut control, &mut revision),
            0
        );
        assert_ne!(
            control & SE_DACL_PROTECTED,
            0,
            "DACL must not inherit access"
        );
        assert!(!dacl.is_null(), "a null DACL allows everyone access");
        assert_eq!(
            (*dacl).AceCount,
            2,
            "only owner rights and SYSTEM may remain"
        );
        let mut principals = Vec::new();
        for index in 0..u32::from((*dacl).AceCount) {
            let mut ace = ptr::null_mut();
            assert_ne!(GetAce(dacl, index, &mut ace), 0);
            let ace = &*ace.cast::<ACCESS_ALLOWED_ACE>();
            assert_eq!(ace.Header.AceType, 0, "expected an access-allowed ACE");
            assert_eq!(u32::from(ace.Header.AceFlags) & INHERITED_ACE, 0);
            assert_eq!(ace.Mask & FILE_ALL_ACCESS, FILE_ALL_ACCESS);
            let sid = ptr::addr_of!(ace.SidStart).cast_mut().cast();
            let mut text = ptr::null_mut();
            assert_ne!(ConvertSidToStringSidW(sid, &mut text), 0);
            let _text = LocalAllocation(text.cast());
            let mut length = 0;
            while *text.add(length) != 0 {
                length += 1;
            }
            principals.push(String::from_utf16(std::slice::from_raw_parts(text, length)).unwrap());
        }
        principals.sort();
        assert_eq!(principals, ["S-1-3-4", "S-1-5-18"]);
    }
}

#[test]
fn new_settings_do_not_inherit_public_access_from_the_parent() {
    let directory = tempfile::tempdir().unwrap();
    allow_inherited_public_access(directory.path());
    let path = directory.path().join("settings.json");

    settings_storage::write_atomic(&path, b"private settings").unwrap();

    assert_eq!(fs::read(&path).unwrap(), b"private settings");
    assert_private_acl(&path);
}

#[test]
fn replacing_existing_settings_removes_inherited_public_access() {
    let directory = tempfile::tempdir().unwrap();
    allow_inherited_public_access(directory.path());
    let path = directory.path().join("settings.json");
    fs::write(&path, b"old settings").unwrap();

    for contents in [
        b"private settings".as_slice(),
        b"updated settings".as_slice(),
    ] {
        settings_storage::write_atomic(&path, contents).unwrap();
        assert_eq!(fs::read(&path).unwrap(), contents);
        assert_private_acl(&path);
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}
